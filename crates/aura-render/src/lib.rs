//! Draws particle instances to a transparent surface with premultiplied alpha.

use aura_effects::{
    Instance, MAX_INSTANCES, MAX_MASKS, Mask,
    sprites::{MAX_SPRITES, atlas_levels, uv_rects},
};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use std::{
    ffi::c_void,
    fmt,
    mem::size_of,
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const MAX_FAILED_ACQUIRES: u32 = 2;

#[derive(Debug)]
pub struct RenderError(String);

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RenderError {}

fn err(e: impl fmt::Display) -> RenderError {
    RenderError(e.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameResult {
    Presented,
    Skipped,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    masks: wgpu::Buffer,
    // Kept alive for the bind group.
    _atlas: wgpu::Texture,
    viewport: Option<[f32; 2]>,
    /// The window masks last uploaded, so unchanged ones are not sent again.
    uploaded_masks: Option<Vec<Mask>>,
    adapter_name: String,
    failed_acquires: u32,
    failed: Arc<AtomicBool>,
}

/// Uploads the visitors' sprite atlas with its mip levels.
fn upload_atlas(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<wgpu::Texture, RenderError> {
    let levels = atlas_levels().map_err(RenderError)?;
    let base = &levels[0];
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas"),
        size: wgpu::Extent3d {
            width: base.width,
            height: base.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (mip, level) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(level.width * 4),
                rows_per_image: Some(level.height),
            },
            wgpu::Extent3d {
                width: level.width,
                height: level.height,
                depth_or_array_layers: 1,
            },
        );
    }
    Ok(texture)
}

impl Renderer {
    /// # Safety
    /// `display` and `surface` must be live Wayland objects that outlive the renderer.
    pub unsafe fn new_wayland(
        display: NonNull<c_void>,
        surface: NonNull<c_void>,
        width: u32,
        height: u32,
        allow_software: bool,
    ) -> Result<Self, RenderError> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(desc);
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                display,
            ))),
            raw_window_handle: RawWindowHandle::Wayland(WaylandWindowHandle::new(surface)),
        };
        // SAFETY: the caller guarantees both handles outlive the renderer.
        let surface = unsafe { instance.create_surface_unsafe(target) }.map_err(err)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))
        .map_err(err)?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu && !allow_software {
            return Err(RenderError(format!(
                "only a software renderer is available ({}), which would waste battery",
                info.name
            )));
        }
        let required_limits = wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits());
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("larkaura"),
            required_limits,
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))
        .map_err(err)?;

        // wgpu panics on unhandled errors. Losing the compositor during logout or
        // a display change must end the renderer cleanly instead.
        let failed = Arc::new(AtomicBool::new(false));
        let flag = failed.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            eprintln!("larkaura-engine: graphics error: {error}");
            flag.store(true, Ordering::Relaxed);
        }));

        let caps = surface.get_capabilities(&adapter);
        let format = [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8Unorm,
        ]
        .into_iter()
        .find(|f| caps.formats.contains(f))
        .ok_or_else(|| RenderError("no 8-bit non-sRGB surface format".into()))?;
        if !caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            return Err(RenderError(
                "the surface does not support premultiplied alpha".into(),
            ));
        }
        // FIFO stalled on GNOME 50.6 in 8 of 48 test launches, waiting for images
        // the compositor had not released. Mailbox never waits, and frame
        // callbacks already limit how often the engine draws.
        let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: Default::default(),
            width: width.max(1),
            height: height.max(1),
            present_mode,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: Vec::new(),
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (MAX_INSTANCES * size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let masks = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("masks"),
            size: MASK_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sprite_table = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprites"),
            size: (MAX_SPRITES * 16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&sprite_table, 0, bytemuck::cast_slice(&uv_rects()));
        let atlas = upload_atlas(&device, &queue)?;
        let atlas_view = atlas.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                uniform(1, wgpu::ShaderStages::VERTEX),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform(4, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: sprite_table.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: masks.as_entire_binding(),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        // Matches `Instance`: center, half size, rotation, shape, extra, color, layer.
        let attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 16,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 20,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 24,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 48,
                shader_location: 6,
            },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particles"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            globals,
            bind_group,
            instances,
            masks,
            _atlas: atlas,
            viewport: None,
            uploaded_masks: None,
            adapter_name: format!("{} ({:?})", info.name, info.backend),
            failed_acquires: 0,
            failed,
        })
    }

    /// True after a graphics error that this renderer cannot recover from.
    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if (width, height) != (self.config.width, self.config.height) {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Draws one frame. `viewport` is the simulation's coordinate space, which
    /// may be larger than the buffer when rendering at reduced resolution.
    /// `masks` are windows that hide instances by layer. `before_present`
    /// runs right before the surface is committed.
    pub fn render(
        &mut self,
        instances: &[Instance],
        masks: &[Mask],
        viewport: [f32; 2],
        before_present: impl FnOnce(),
    ) -> FrameResult {
        let (frame, suboptimal) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => (t, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => (t, true),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return FrameResult::Skipped;
            }
            _ => {
                // Rebuilding the swapchain frees images that are stuck elsewhere.
                self.failed_acquires += 1;
                if self.failed_acquires >= MAX_FAILED_ACQUIRES {
                    self.failed_acquires = 0;
                    self.surface.configure(&self.device, &self.config);
                }
                return FrameResult::Skipped;
            }
        };
        self.failed_acquires = 0;
        let count = instances.len().min(MAX_INSTANCES);
        // Every upload costs a staging buffer, so skip the ones that change nothing.
        let masks = &masks[..masks.len().min(MAX_MASKS)];
        let masks_changed = self.uploaded_masks.as_deref() != Some(masks);
        if self.viewport != Some(viewport) || masks_changed {
            self.viewport = Some(viewport);
            let globals: [f32; 4] = [
                viewport[0].max(1.0),
                viewport[1].max(1.0),
                masks.len() as f32,
                0.0,
            ];
            self.queue
                .write_buffer(&self.globals, 0, bytemuck::cast_slice(&globals));
        }
        if masks_changed {
            self.queue
                .write_buffer(&self.masks, 0, bytemuck::cast_slice(&mask_table(masks)));
            self.uploaded_masks = Some(masks.to_vec());
        }
        if count > 0 {
            self.queue.write_buffer(
                &self.instances,
                0,
                bytemuck::cast_slice(&instances[..count]),
            );
        }
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("particles"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if count > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.instances.slice(..));
                pass.draw(0..4, 0..count as u32);
            }
        }
        self.queue.submit([encoder.finish()]);
        before_present();
        self.queue.present(frame);
        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
        FrameResult::Presented
    }
}

/// The mask uniform: every rectangle, then the alphas four to a vector.
const MASK_BYTES: usize = (MAX_MASKS + MAX_MASKS / 4) * 16;

fn mask_table(masks: &[Mask]) -> [f32; MASK_BYTES / 4] {
    let mut out = [0.0; MASK_BYTES / 4];
    for (i, m) in masks.iter().take(MAX_MASKS).enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&m.rect);
        out[MAX_MASKS * 4 + i] = m.alpha;
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn shader_parses_and_validates() {
        let source = include_str!("shader.wgsl");
        let module = naga::front::wgsl::parse_str(source).expect("shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("shader validates");
    }

    #[test]
    fn instance_layout_matches_the_vertex_attributes() {
        use aura_effects::Instance;
        use std::mem::offset_of;
        assert_eq!(size_of::<Instance>(), 52);
        assert_eq!(offset_of!(Instance, rotation), 16);
        assert_eq!(offset_of!(Instance, shape), 20);
        assert_eq!(offset_of!(Instance, extra), 24);
        assert_eq!(offset_of!(Instance, color), 32);
        assert_eq!(offset_of!(Instance, layer), 48);
    }

    #[test]
    fn mask_table_matches_the_shader_layout() {
        use super::*;
        let masks = [
            Mask {
                rect: [1.0, 2.0, 3.0, 4.0],
                alpha: 1.0,
            },
            Mask {
                rect: [5.0, 6.0, 7.0, 8.0],
                alpha: 0.5,
            },
        ];
        let t = mask_table(&masks);
        assert_eq!(&t[..8], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        assert_eq!(&t[MAX_MASKS * 4..MAX_MASKS * 4 + 3], &[1.0, 0.5, 0.0]);
        let source = include_str!("shader.wgsl");
        assert!(source.contains(&format!("const MAX_MASKS: u32 = {MAX_MASKS}u;")));
        assert!(source.contains(&format!("const MAX_SPRITES: u32 = {MAX_SPRITES}u;")));
    }
}
