struct Globals {
    viewport: vec2f,
    // How many window masks are in use.
    mask_count: f32,
    pad: f32,
}

// Keep in step with aura_effects::sprites::MAX_SPRITES and MAX_MASKS.
const MAX_SPRITES: u32 = 64u;
const MAX_MASKS: u32 = 32u;
const SHAPE_SPRITE: f32 = 32.0;

// Windows that hide effects: x, y, width, height in logical pixels, and
// how strongly each hides, four to a vector.
struct Masks {
    rects: array<vec4f, MAX_MASKS>,
    alphas: array<vec4f, 8>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
// Each atlas frame as normalized x, y, width, height.
@group(0) @binding(1) var<uniform> sprites: array<vec4f, MAX_SPRITES>;
@group(0) @binding(2) var atlas: texture_2d<f32>;
@group(0) @binding(3) var atlas_sampler: sampler;
@group(0) @binding(4) var<uniform> masks: Masks;

struct VertexIn {
    @builtin(vertex_index) index: u32,
    @location(0) center: vec2f,
    @location(1) half_size: vec2f,
    @location(2) rotation: f32,
    @location(3) shape: f32,
    @location(4) color: vec4f,
    @location(5) extra: vec2f,
    @location(6) layer: f32,
}

struct VertexOut {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) @interpolate(flat) shape: f32,
    @location(2) color: vec4f,
    @location(3) atlas_uv: vec2f,
    // extra.xy, then the half size in pixels.
    @location(4) @interpolate(flat) params: vec4f,
    // Logical pixel position, for window masks.
    @location(5) world: vec2f,
    @location(6) @interpolate(flat) layer: f32,
}

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    let uv = vec2f(f32(in.index & 1u) * 2.0 - 1.0, f32((in.index >> 1u) & 1u) * 2.0 - 1.0);
    let local = uv * in.half_size;
    let c = cos(in.rotation);
    let s = sin(in.rotation);
    let world = in.center + vec2f(local.x * c - local.y * s, local.x * s + local.y * c);
    let ndc = world / globals.viewport * 2.0 - 1.0;
    var out: VertexOut;
    out.position = vec4f(ndc.x, -ndc.y, 0.0, 1.0);
    out.uv = uv;
    out.shape = in.shape;
    out.color = in.color;
    out.atlas_uv = vec2f(0.0);
    if in.shape > SHAPE_SPRITE - 0.5 {
        let frame = min(u32(in.shape - SHAPE_SPRITE + 0.5), MAX_SPRITES - 1u);
        let r = sprites[frame];
        out.atlas_uv = r.xy + (uv * 0.5 + 0.5) * r.zw;
    }
    out.params = vec4f(in.extra, abs(in.half_size));
    out.world = world;
    out.layer = in.layer;
    return out;
}

// How much of a fragment shows past the windows that may hide it: those
// from `layer` up. Edges are antialiased over about a logical pixel.
fn unmasked(p: vec2f, layer: f32) -> f32 {
    let count = min(u32(globals.mask_count + 0.5), MAX_MASKS);
    var keep = 1.0;
    for (var i = u32(max(layer, 0.0) + 0.5); i < count; i++) {
        let r = masks.rects[i];
        let d = min(min(p.x - r.x, r.x + r.z - p.x), min(p.y - r.y, r.y + r.w - p.y));
        let alpha = masks.alphas[i / 4u][i % 4u];
        keep *= 1.0 - clamp(d + 0.5, 0.0, 1.0) * alpha;
        if keep <= 0.0 {
            break;
        }
    }
    return keep;
}

// Interleaved gradient noise hides banding in faint, wide gradients.
fn dither(p: vec2f) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2f(0.06711056, 0.00583715)))) - 0.5;
}

// Antialiased inside test: `d` is positive inside the shape.
fn edge(d: f32, reference: f32) -> f32 {
    return clamp(d / max(fwidth(reference), 0.0001) + 0.5, 0.0, 1.0);
}

fn shape_alpha(shape: u32, uv: vec2f, params: vec4f) -> f32 {
    let r2 = dot(uv, uv);
    let soft = clamp(1.0 - r2, 0.0, 1.0);
    switch shape {
        case 0u: { return soft * soft; }
        case 1u: {
            let across = clamp(1.0 - abs(uv.x), 0.0, 1.0);
            let along = clamp((uv.y + 1.0) * 0.5, 0.0, 1.0);
            return across * along * (0.45 + 0.55 * along);
        }
        case 2u: { return soft * soft * soft; }
        case 3u: {
            let core = clamp(1.0 - r2 * 18.0, 0.0, 1.0);
            return soft * soft * soft * 0.42 + core;
        }
        case 4u: {
            // Leaf: pointed at both ends, with a faint center vein.
            let half_width = 1.0 - uv.y * uv.y;
            let vein = 1.0 - 0.28 * clamp(1.0 - abs(uv.x) * 12.0, 0.0, 1.0);
            return edge(half_width - abs(uv.x), uv.x) * vein;
        }
        case 5u: {
            // Petal: round, narrowing to the base, with a small notch at the tip.
            let t = clamp((uv.y + 1.0) * 0.5, 0.0, 1.0);
            let half_width = sqrt(clamp(1.0 - uv.y * uv.y, 0.0, 1.0)) * (1.0 - 0.55 * t);
            let notch = 0.16 - length(uv - vec2f(0.0, -1.02)) * 0.8;
            return edge(min(half_width - abs(uv.x), -notch), uv.x);
        }
        case 6u: { return edge(1.0 - length(uv), uv.x + uv.y); }
        case 7u: { return edge(1.0 - max(abs(uv.x), abs(uv.y)), uv.x + uv.y); }
        case 8u: { return edge(min((uv.y + 1.0) * 0.5 - abs(uv.x), 1.0 - uv.y), uv.x + uv.y); }
        case 9u: {
            // Four-pointed sparkle.
            let a = sqrt(abs(uv));
            return edge(1.0 - a.x - a.y, uv.x + uv.y);
        }
        case 10u: {
            // Dome: the upper half of a disk, used for piled snow.
            if uv.y > 0.0 { return 0.0; }
            return edge(1.0 - length(uv), uv.x + uv.y);
        }
        case 12u: {
            // Raindrop: solid across its width, fading toward the tail.
            let across = clamp(1.0 - abs(uv.x), 0.0, 1.0);
            let along = clamp((uv.y + 1.0) * 0.5, 0.0, 1.0);
            return sqrt(across) * along * (0.6 + 0.4 * along);
        }
        case 13u: {
            // Fluffy snowflake: a soft-feathered bright core inside a delicate glow.
            // extra.x is how far out of focus it is, from 0 (sharp) to 1 (soft bokeh disk).
            let blur = clamp(params.x, 0.0, 1.0);
            let r = length(uv);
            let radius = mix(0.48, 0.80, blur);
            let px = max(fwidth(r), 0.0001);
            let width = max(px * 1.3, mix(0.14, 0.46, blur));
            let disk = clamp((radius - r) / width + 0.5, 0.0, 1.0) * mix(1.0, 0.65, blur);
            let halo = pow(clamp(1.0 - r, 0.0, 1.0), 2.1) * 0.42 * (1.0 - blur * 0.7);
            return max(disk, halo);
        }
        case 14u: {
            // Snowbank: a column whose top runs from extra.x (left) to
            // extra.y (right), as shares of the column's height.
            let u = uv.x * 0.5 + 0.5;
            let up = 0.5 - uv.y * 0.5;
            let top = mix(params.x, params.y, u);
            return edge(top - up, uv.y);
        }
        case 15u: {
            // Tumbling snow clump: organic lumpy silhouette with soft feathered edge.
            let a = atan2(uv.y, uv.x);
            let lump = 1.0 + 0.13 * sin(a * 3.0 + params.x) + 0.08 * cos(a * 5.0 - params.x * 1.7);
            let r = length(uv) / lump;
            let px = max(fwidth(r), 0.0001);
            return clamp((0.86 - r) / max(px * 1.4, 0.12) + 0.5, 0.0, 1.0);
        }
        default: {
            let across = clamp(1.0 - abs(uv.x), 0.0, 1.0);
            return across * across;
        }
    }
}

// Light on a snowbank column. A bright rim runs along the top, slopes
// facing the light (from the upper left) are lighter and the others cooler,
// and the snow turns a cool blue deeper down, which gives deep drifts volume.
fn snowbank_tone(uv: vec2f, params: vec4f) -> vec3f {
    let u = uv.x * 0.5 + 0.5;
    let up = 0.5 - uv.y * 0.5;
    let below = (mix(params.x, params.y, u) - up) * params.w * 2.0;
    // Rise in pixels per pixel across the column; positive faces left.
    let slope = clamp((params.y - params.x) * params.w / max(params.z, 0.5), -1.5, 1.5);
    let rim = clamp(1.0 - below / 3.0, 0.0, 1.0);
    let lit = clamp(1.0 - below / 15.0, 0.0, 1.0);
    let mid = clamp(below / 42.0, 0.0, 1.0);
    let deep = clamp(below / 115.0, 0.0, 1.0);
    let upper = mix(vec3f(0.94, 0.96, 1.0), vec3f(0.84, 0.89, 0.98), mid);
    let body = mix(upper, vec3f(0.69, 0.76, 0.93), deep);
    // Keep directional slope highlights near the sculpted sunlit crest so deep
    // drifts stay silky smooth with no vertical column banding.
    let crest = clamp(1.0 - below / 28.0, 0.0, 1.0);
    let facing = slope * 0.08 * crest;
    let tone = mix(body * (1.0 + facing), vec3f(1.0), lit * 0.72 + rim * 0.28);
    return clamp(tone, vec3f(0.0), vec3f(1.0));
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    // Sampled before any branch, as WGSL requires for implicit derivatives.
    let texel = textureSample(atlas, atlas_sampler, in.atlas_uv);
    let keep = unmasked(in.world, in.layer);
    if in.shape > SHAPE_SPRITE - 0.5 {
        return vec4f(texel.rgb * in.color.rgb, texel.a * in.color.a) * keep;
    }
    let shape = u32(in.shape + 0.5);
    let k = shape_alpha(shape, in.uv, in.params);
    var out = in.color * clamp(k, 0.0, 1.0);
    if shape == 12u {
        // A pale core over the darker body shows on light and dark wallpapers.
        let core = clamp(1.0 - abs(in.uv.x) * 2.2, 0.0, 1.0);
        out = vec4f(mix(out.rgb, vec3f(out.a), core * 0.7), out.a);
    }
    if shape == 14u {
        out = vec4f(out.rgb * snowbank_tone(in.uv, in.params), out.a);
    }
    if shape == 15u {
        let lit = clamp(0.68 - 0.36 * (in.uv.x * 0.6 + in.uv.y * 0.8), 0.0, 1.0);
        let shade = mix(vec3f(0.74, 0.80, 0.95), vec3f(1.0, 1.0, 1.0), lit);
        out = vec4f(out.rgb * shade, out.a);
    }
    out *= keep;
    let a = clamp(out.a + dither(in.position.xy) / 255.0, 0.0, 1.0);
    return vec4f(min(out.rgb, vec3f(a)), a);
}
