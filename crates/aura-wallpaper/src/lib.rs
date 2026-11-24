//! Wallpaper library: original generated scenes, imported images and, on GNOME,
//! a native time-of-day wallpaper that costs nothing to run.

mod scene;

pub use scene::{Moment, Theme, render_scene};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

const MAX_IMPORT_BYTES: u64 = 64 * 1024 * 1024;
const SCENE_SIZE: (u32, u32) = (2560, 1440);
const PREVIEW_SIZE: (u32, u32) = (480, 270);
/// Seconds each moment lasts, starting at 05:00. Together they make one day.
const DAY_PLAN: [u32; 8] = [5400, 5400, 10_800, 18_000, 7200, 3600, 5400, 30_600];
const FADE_SECONDS: u32 = 1800;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Wallpaper {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    /// Image shown as the preview. For a theme this is its midday view.
    pub preview: PathBuf,
    /// One preview per time of day, for generated themes.
    pub moments: Vec<MomentPreview>,
    pub dynamic: bool,
    pub imported: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MomentPreview {
    pub id: &'static str,
    pub name: &'static str,
    pub preview: PathBuf,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
struct Backup {
    uri: String,
    uri_dark: String,
    options: String,
}

pub struct Library {
    root: PathBuf,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && !id.starts_with('.')
        && !id.contains("..")
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Recognizes images by their contents rather than their file name.
fn image_extension(header: &[u8]) -> Option<&'static str> {
    if header.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if header.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if header.len() >= 12 && &header[..4] == b"RIFF" && &header[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

pub fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~') {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A 24-hour GNOME background that cross-fades between the moments.
pub fn dynamic_xml(files: &[(Moment, PathBuf)]) -> String {
    let mut xml = String::from(
        "<background>\n  <starttime><year>2026</year><month>1</month><day>1</day>\
         <hour>5</hour><minute>0</minute><second>0</second></starttime>\n",
    );
    for (i, (_, file)) in files.iter().enumerate() {
        let current = escape_xml(&file.to_string_lossy());
        let next = escape_xml(&files[(i + 1) % files.len()].1.to_string_lossy());
        let hold = DAY_PLAN[i % DAY_PLAN.len()] - FADE_SECONDS;
        xml.push_str(&format!(
            "  <static><duration>{hold}</duration><file>{current}</file></static>\n  \
             <transition type=\"overlay\"><duration>{FADE_SECONDS}</duration>\
             <from>{current}</from><to>{next}</to></transition>\n"
        ));
    }
    xml.push_str("</background>\n");
    xml
}

fn encode_png(rgb: &[u8], (w, h): (u32, u32)) -> io::Result<Vec<u8>> {
    let mut encoded = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded, w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Balanced);
    let mut writer = encoder.write_header().map_err(io::Error::other)?;
    writer.write_image_data(rgb).map_err(io::Error::other)?;
    writer.finish().map_err(io::Error::other)?;
    Ok(encoded)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)
}

fn gsettings(args: &[&str]) -> io::Result<String> {
    let output = Command::new("gsettings").args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Reads a GVariant string as printed by `gsettings get`.
fn unquote(value: &str) -> String {
    let inner = value
        .strip_prefix('\'')
        .and_then(|v| v.strip_suffix('\''))
        .unwrap_or(value);
    inner.replace("\\'", "'").replace("\\\\", "\\")
}

const SCHEMA: &str = "org.gnome.desktop.background";

fn read_background(key: &str) -> io::Result<String> {
    gsettings(&["get", SCHEMA, key]).map(|v| unquote(&v))
}

fn write_background(backup: &Backup) -> io::Result<()> {
    gsettings(&["set", SCHEMA, "picture-options", &backup.options])?;
    gsettings(&["set", SCHEMA, "picture-uri", &backup.uri])?;
    gsettings(&["set", SCHEMA, "picture-uri-dark", &backup.uri_dark])?;
    Ok(())
}

impl Library {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_root() -> Option<PathBuf> {
        let absolute = |p: PathBuf| p.is_absolute().then_some(p);
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .and_then(absolute)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|h| PathBuf::from(h).join(".local/share"))
                    .and_then(absolute)
            })
            .map(|p| p.join("larkaura").join("wallpapers"))
    }

    /// Raise the number when the artwork changes, so finished images are made
    /// again. Older folders stay, because the desktop may still be showing one.
    fn scenes(&self) -> PathBuf {
        self.root.join("scenes-4")
    }

    fn imported(&self) -> PathBuf {
        self.root.join("imported")
    }

    fn scene_file(&self, theme: Theme, moment: Moment) -> PathBuf {
        self.scenes()
            .join(format!("{}-{}.png", theme.id(), moment.id()))
    }

    fn preview_file(&self, theme: Theme, moment: Moment) -> PathBuf {
        self.scenes()
            .join(format!("preview-{}-{}.png", theme.id(), moment.id()))
    }

    fn day_file(&self, theme: Theme) -> PathBuf {
        self.scenes().join(format!("{}-day.xml", theme.id()))
    }

    fn ensure_image(
        &self,
        file: &Path,
        theme: Theme,
        moment: Moment,
        size: (u32, u32),
    ) -> io::Result<()> {
        if !file.exists() {
            let rgb = render_scene(theme, moment, size.0, size.1);
            write_atomic(file, &encode_png(&rgb, size)?)?;
        }
        Ok(())
    }

    /// Draws several images at once, one thread per processor.
    fn ensure_many(&self, jobs: &[(PathBuf, Theme, Moment)], size: (u32, u32)) -> io::Result<()> {
        let missing: Vec<_> = jobs.iter().filter(|(file, ..)| !file.exists()).collect();
        if missing.is_empty() {
            return Ok(());
        }
        let threads = std::thread::available_parallelism()
            .map_or(2, |n| n.get())
            .clamp(1, 8);
        let next = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads.min(missing.len()))
                .map(|_| {
                    scope.spawn(|| -> io::Result<()> {
                        loop {
                            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some((file, theme, moment)) = missing.get(i) else {
                                return Ok(());
                            };
                            self.ensure_image(file, *theme, *moment, size)?;
                        }
                    })
                })
                .collect();
            // The scope waits for every worker, even after the first error.
            workers.into_iter().try_for_each(|w| {
                w.join()
                    .unwrap_or_else(|_| Err(io::Error::other("drawing failed")))
            })
        })
    }

    /// Small previews are made up front. Full images wait until they are used.
    pub fn ensure_previews(&self) -> io::Result<()> {
        fs::create_dir_all(self.scenes())?;
        let jobs: Vec<_> = Theme::ALL
            .into_iter()
            .flat_map(|theme| Moment::ALL.into_iter().map(move |moment| (theme, moment)))
            .map(|(theme, moment)| (self.preview_file(theme, moment), theme, moment))
            .collect();
        self.ensure_many(&jobs, PREVIEW_SIZE)
    }

    fn ensure_full(&self, theme: Theme, moment: Moment) -> io::Result<PathBuf> {
        fs::create_dir_all(self.scenes())?;
        let file = self.scene_file(theme, moment);
        self.ensure_image(&file, theme, moment, SCENE_SIZE)?;
        Ok(file)
    }

    fn ensure_day(&self, theme: Theme) -> io::Result<PathBuf> {
        fs::create_dir_all(self.scenes())?;
        let jobs: Vec<_> = Moment::ALL
            .into_iter()
            .map(|moment| (self.scene_file(theme, moment), theme, moment))
            .collect();
        self.ensure_many(&jobs, SCENE_SIZE)?;
        let files: Vec<_> = jobs
            .into_iter()
            .map(|(file, _, moment)| (moment, file))
            .collect();
        let file = self.day_file(theme);
        write_atomic(&file, dynamic_xml(&files).as_bytes())?;
        Ok(file)
    }

    pub fn list(&self) -> io::Result<Vec<Wallpaper>> {
        self.ensure_previews()?;
        let mut items: Vec<Wallpaper> = Theme::ALL
            .into_iter()
            .map(|theme| Wallpaper {
                id: theme.id().into(),
                name: theme.name().into(),
                path: self.day_file(theme),
                preview: self.preview_file(theme, Moment::Midday),
                moments: Moment::ALL
                    .into_iter()
                    .map(|m| MomentPreview {
                        id: m.id(),
                        name: m.name(),
                        preview: self.preview_file(theme, m),
                    })
                    .collect(),
                dynamic: true,
                imported: false,
            })
            .collect();
        if let Ok(entries) = fs::read_dir(self.imported()) {
            let mut imported: Vec<_> = entries
                .flatten()
                .filter_map(|e| {
                    let id = e.file_name().into_string().ok()?;
                    (valid_id(&id) && e.path().is_file()).then(|| Wallpaper {
                        name: id
                            .split_once('-')
                            .map_or(id.as_str(), |(_, n)| n)
                            .to_string(),
                        path: e.path(),
                        preview: e.path(),
                        moments: Vec::new(),
                        dynamic: false,
                        imported: true,
                        id,
                    })
                })
                .collect();
            imported.sort_by(|a, b| a.name.cmp(&b.name));
            items.extend(imported);
        }
        Ok(items)
    }

    /// Copies an image into the library after checking what it really is.
    pub fn import(&self, source: &Path) -> io::Result<Wallpaper> {
        let meta = fs::metadata(source)?;
        if !meta.is_file() {
            return Err(invalid("that is not a file"));
        }
        if meta.len() > MAX_IMPORT_BYTES {
            return Err(invalid("that image is larger than 64 MB"));
        }
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        fs::File::open(source)?
            .take(MAX_IMPORT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        let extension = image_extension(&bytes)
            .ok_or_else(|| invalid("only PNG, JPEG and WebP images are supported"))?;
        let stem: String = source
            .file_stem()
            .map(|s| {
                s.to_string_lossy()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                    .take(48)
                    .collect()
            })
            .unwrap_or_default();
        let stem = stem.trim_matches('-');
        let id = format!(
            "{:012x}-{}.{extension}",
            fnv1a(&bytes) & 0xffff_ffff_ffff,
            if stem.is_empty() { "image" } else { stem }
        );
        fs::create_dir_all(self.imported())?;
        write_atomic(&self.imported().join(&id), &bytes)?;
        self.find(&id)
    }

    pub fn find(&self, id: &str) -> io::Result<Wallpaper> {
        if !valid_id(id) {
            return Err(invalid("unknown wallpaper"));
        }
        self.list()?
            .into_iter()
            .find(|w| w.id == id)
            .ok_or_else(|| invalid("unknown wallpaper"))
    }

    pub fn remove_imported(&self, id: &str) -> io::Result<()> {
        let item = self.find(id)?;
        if !item.imported {
            return Err(invalid("built-in wallpapers cannot be removed"));
        }
        fs::remove_file(item.path)
    }

    fn backup_path(&self) -> PathBuf {
        self.root.join("previous-wallpaper.json")
    }

    pub fn has_backup(&self) -> bool {
        self.backup_path().exists()
    }

    /// The file to show: a theme at one moment, a theme through the whole day,
    /// or an imported image.
    pub fn prepare(&self, id: &str, moment: Option<&str>) -> io::Result<PathBuf> {
        let item = self.find(id)?;
        let Some(theme) = Theme::from_id(&item.id) else {
            return Ok(item.path);
        };
        match moment {
            Some(m) => {
                let moment = Moment::from_id(m).ok_or_else(|| invalid("unknown time of day"))?;
                self.ensure_full(theme, moment)
            }
            None => self.ensure_day(theme),
        }
    }

    /// Sets a GNOME wallpaper. The first use remembers the user's own choice.
    pub fn apply_gnome(&self, id: &str, moment: Option<&str>) -> io::Result<()> {
        let path = self.prepare(id, moment)?;
        fs::create_dir_all(&self.root)?;
        if !self.has_backup() {
            let backup = Backup {
                uri: read_background("picture-uri")?,
                uri_dark: read_background("picture-uri-dark")?,
                options: read_background("picture-options")?,
            };
            write_atomic(&self.backup_path(), &serde_json::to_vec(&backup)?)?;
        }
        let uri = file_uri(&path);
        write_background(&Backup {
            uri: uri.clone(),
            uri_dark: uri,
            options: "zoom".into(),
        })
    }

    /// Puts back the wallpaper the user had before LarkAura changed it.
    pub fn restore_gnome(&self) -> io::Result<()> {
        let backup: Backup = serde_json::from_slice(&fs::read(self.backup_path())?)?;
        write_background(&backup)?;
        fs::remove_file(self.backup_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(name: &str) -> (Library, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("larkaura-wall-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        (Library::new(root.join("lib")), root)
    }

    #[test]
    fn uris_are_encoded() {
        assert_eq!(
            file_uri(Path::new("/a b/c'd#.png")),
            "file:///a%20b/c%27d%23.png"
        );
    }

    #[test]
    fn reads_gsettings_strings() {
        assert_eq!(unquote("'file:///a\\'b.png'"), "file:///a'b.png");
    }

    #[test]
    fn dynamic_day_lasts_exactly_24_hours_and_escapes_paths() {
        let files: Vec<_> = Moment::ALL
            .iter()
            .map(|m| (*m, PathBuf::from(format!("/a&b/{}.png", m.id()))))
            .collect();
        let xml = dynamic_xml(&files);
        let total: u32 = xml
            .split("<duration>")
            .skip(1)
            .map(|p| p.split('<').next().unwrap().parse::<u32>().unwrap())
            .sum();
        assert_eq!(total, 86_400);
        assert!(xml.contains("/a&amp;b/dawn.png") && !xml.contains("/a&b/"));
    }

    #[test]
    fn import_checks_contents_not_names() {
        let (lib, root) = library("import");
        let fake = root.join("fake.png");
        fs::write(&fake, b"this is not an image").unwrap();
        assert!(lib.import(&fake).is_err());
        let real = root.join("My Photo!.jpeg");
        fs::write(&real, [0xff, 0xd8, 0xff, 0xe0, 1, 2, 3]).unwrap();
        let item = lib.import(&real).unwrap();
        assert!(item.imported && item.id.ends_with("-My-Photo.jpg"));
        assert!(item.path.starts_with(lib.imported()));
        lib.remove_imported(&item.id).unwrap();
        assert!(lib.find(&item.id).is_err());
    }

    #[test]
    fn ids_cannot_escape_the_library() {
        let (lib, _) = library("escape");
        for id in ["../../secret", "a/b", "..", ".hidden", ""] {
            assert!(lib.find(id).is_err(), "{id}");
        }
        assert!(lib.remove_imported("hills").is_err());
    }

    #[test]
    fn themes_list_every_moment_and_reject_unknown_ones() {
        let (lib, _) = library("themes");
        let items = lib.list().unwrap();
        assert_eq!(items.len(), Theme::ALL.len());
        assert!(
            items
                .iter()
                .all(|w| w.moments.len() == 8 && w.moments.iter().all(|m| m.preview.is_file()))
        );
        assert!(lib.prepare("snowline", Some("teatime")).is_err());
        assert!(!lib.scene_file(Theme::Snowline, Moment::Dawn).exists());
    }
}
