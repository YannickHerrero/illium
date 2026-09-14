//! Local, data-only theme packs. No downloads, scripts or recursive link traversal.
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_IMAGES: usize = 64;
pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
// Includes slightly oversized 8K artwork such as Dracula's 8001×4501 base.png.
pub const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_PACK_BYTES: u64 = 512 * 1024 * 1024;

pub fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 240
        && !name.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
        && !name.chars().any(char::is_control)
        && !name.ends_with(['.', ' '])
        && !matches!(name, "." | "..")
}
fn pack_id(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
}
fn metadata(path: &Path, directory: bool) -> Result<fs::Metadata, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(windows)]
    let link = {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let link = meta.file_type().is_symlink();
    if link || (directory && !meta.is_dir()) || (!directory && !meta.is_file()) {
        return Err(format!(
            "{}: expected a regular non-reparse {}",
            path.display(),
            if directory { "directory" } else { "file" }
        ));
    }
    Ok(meta)
}
fn read(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    if metadata(path, false)?.len() > max as u64 {
        return Err(format!("{}: file too large", path.display()));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("file too large".into());
    }
    Ok(bytes)
}
pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        ["jpg", "jpeg", "png"]
            .iter()
            .any(|ext| s.eq_ignore_ascii_case(ext))
    })
}
/// Sorted names only; pixels are decoded lazily when selected. Broken files remain
/// discoverable, so an explicit selection can report an error rather than vanish.
pub fn images(dir: &Path) -> Result<Vec<String>, String> {
    if !dir.try_exists().map_err(|e| e.to_string())? {
        return Ok(vec![]);
    }
    metadata(dir, true)?;
    let mut names = Vec::new();
    for (n, entry) in fs::read_dir(dir).map_err(|e| e.to_string())?.enumerate() {
        if n >= 256 {
            return Err("wallpaper directory exceeds 256 entries".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !is_image(&path) {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if plain_name(&name) && metadata(&path, false).is_ok() {
            names.push(name);
        }
    }
    if names.len() > MAX_IMAGES {
        return Err("theme exceeds 64 wallpapers".into());
    }
    names.sort();
    Ok(names)
}
pub fn wallpaper_dir(home: &Path, theme: &str) -> Result<PathBuf, String> {
    if !crate::valid_name(theme) || !plain_name(theme) {
        return Err("invalid theme name".into());
    }
    let themes = home.join("themes");
    metadata(&themes, true)?;
    let assets = themes.join(theme);
    if assets.try_exists().map_err(|e| e.to_string())? {
        metadata(&assets, true)?;
    }
    Ok(assets.join("wallpapers"))
}
/// Fingerprints only: watching images never reads their contents into memory.
pub fn fingerprint(home: &Path, theme: &str) -> Result<Vec<(String, u64, u128)>, String> {
    let dir = wallpaper_dir(home, theme)?;
    images(&dir)?
        .into_iter()
        .map(|name| {
            let meta = metadata(&dir.join(&name), false)?;
            let modified = meta
                .modified()
                .map_err(|e| e.to_string())?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            Ok((name, meta.len(), modified))
        })
        .collect()
}
fn decode_bytes(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Jpeg | image::ImageFormat::Png)
    ) {
        return Err("wallpaper must be JPEG or PNG".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (width, height) = image::ImageDecoder::dimensions(&decoder);
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("wallpaper exceeds 64 megapixels or has zero dimensions".into());
    }
    image::DynamicImage::from_decoder(decoder)
        .map(|i| i.to_rgba8())
        .map_err(|e| e.to_string())
}
pub fn decode(path: &Path) -> Result<image::RgbaImage, String> {
    decode_bytes(&read(path, MAX_IMAGE_BYTES)?).map_err(|e| format!("{}: {e}", path.display()))
}
/// Center-crop before resizing, so even a 1-pixel-wide panorama never creates
/// an enormous intermediate buffer. Preparing native-sized pixels also avoids
/// Slint 1.12's fixed-point division by zero when upscaling tiny images >256×.
pub fn cover(
    pixels: &image::RgbaImage,
    width: u32,
    height: u32,
) -> Result<image::RgbaImage, String> {
    if width == 0
        || height == 0
        || width > 16384
        || height > 16384
        || u64::from(width) * u64::from(height) > MAX_PIXELS
        || pixels.width() == 0
        || pixels.height() == 0
    {
        return Err("invalid wallpaper display dimensions".into());
    }
    let (iw, ih) = pixels.dimensions();
    let (cw, ch) = if u64::from(iw) * u64::from(height) > u64::from(ih) * u64::from(width) {
        (
            ((u64::from(ih) * u64::from(width) / u64::from(height)) as u32).max(1),
            ih,
        )
    } else {
        (
            iw,
            ((u64::from(iw) * u64::from(height) / u64::from(width)) as u32).max(1),
        )
    };
    use fast_image_resize::{
        FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
        images::{Image, ImageRef},
    };
    let source =
        ImageRef::new(iw, ih, pixels.as_raw(), PixelType::U8x4).map_err(|e| e.to_string())?;
    let mut destination = Image::new(width, height, PixelType::U8x4);
    // Bilinear convolution is the triangular filter used previously, but SIMD
    // accelerates it. Alpha-aware filtering avoids dark fringes in transparent PNGs.
    let options = ResizeOptions::new()
        .resize_alg(ResizeAlg::Convolution(FilterType::Bilinear))
        .crop(
            f64::from((iw - cw) / 2),
            f64::from((ih - ch) / 2),
            f64::from(cw),
            f64::from(ch),
        );
    Resizer::new()
        .resize(&source, &mut destination, &options)
        .map_err(|e| e.to_string())?;
    image::RgbaImage::from_raw(width, height, destination.into_vec())
        .ok_or_else(|| "invalid resized image buffer".into())
}
/// Installs `<source>/theme.toml` as `themes/<source-name>.toml`, publishing the
/// palette last. Existing themes/assets are never replaced. The source is untouched.
pub fn install(home: &Path, source: &Path) -> Result<String, String> {
    metadata(source, true)?;
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| pack_id(s))
        .ok_or(
            "pack folder name must contain only lowercase ASCII letters, digits, - or _ (1..64)",
        )?;
    let palette = read(&source.join("theme.toml"), 65536)?;
    crate::Theme::parse(std::str::from_utf8(&palette).map_err(|e| e.to_string())?)?;
    let source_images = source.join("wallpapers");
    let names = images(&source_images)?;
    let themes = home.join("themes");
    fs::create_dir_all(&themes).map_err(|e| e.to_string())?;
    metadata(&themes, true)?;
    let target = themes.join(format!("{name}.toml"));
    let assets = themes.join(name);
    if fs::symlink_metadata(&target).is_ok() || fs::symlink_metadata(&assets).is_ok() {
        return Err(format!("theme already exists: {name}"));
    }
    // An owned staging directory, ignored by theme discovery/config snapshots.
    let stage = themes.join(format!(".install-{name}-{}", std::process::id()));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let mut reserved = false;
    let result = (|| -> Result<(), String> {
        fs::write(stage.join("theme.toml"), &palette).map_err(|e| e.to_string())?;
        fs::create_dir(stage.join("wallpapers")).map_err(|e| e.to_string())?;
        let mut total = 0;
        for file in names {
            let bytes = read(&source_images.join(&file), MAX_IMAGE_BYTES)?;
            total += bytes.len() as u64;
            if total > MAX_PACK_BYTES {
                return Err("pack images exceed 512 MiB".into());
            }
            decode_bytes(&bytes).map_err(|e| format!("{file}: {e}"))?;
            fs::write(stage.join("wallpapers").join(file), bytes).map_err(|e| e.to_string())?;
        }
        for doc in ["README.md", "LICENSE", "SOURCES.md"] {
            if source.join(doc).try_exists().map_err(|e| e.to_string())? {
                fs::write(stage.join(doc), read(&source.join(doc), 65536)?)
                    .map_err(|e| e.to_string())?;
            }
        }
        // Reserve without overwriting even an empty pre-existing directory.
        fs::create_dir(&assets).map_err(|e| e.to_string())?;
        reserved = true;
        for file in fs::read_dir(&stage).map_err(|e| e.to_string())? {
            let file = file.map_err(|e| e.to_string())?;
            if file.file_name() == "theme.toml" {
                continue;
            }
            fs::rename(file.path(), assets.join(file.file_name())).map_err(|e| e.to_string())?;
        }
        // Same-volume hard link publishes a complete regular file atomically and
        // fails if the destination exists (unlike rename on Unix). NTFS supported.
        fs::hard_link(stage.join("theme.toml"), &target).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() && reserved {
        let _ = fs::remove_dir_all(&assets);
    }
    let _ = fs::remove_dir_all(stage);
    result.map(|()| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new(name: &str) -> Self {
            let p =
                std::env::temp_dir().join(format!("winarchy-pack-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(p.join("source/mine/wallpapers")).unwrap();
            fs::write(p.join("source/mine/theme.toml"), crate::DEFAULT).unwrap();
            Self(p)
        }
        fn source(&self) -> PathBuf {
            self.0.join("source/mine")
        }
        fn home(&self) -> PathBuf {
            self.0.join("home")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn png(path: &Path) {
        image::RgbaImage::new(2, 2).save(path).unwrap();
    }
    #[test]
    fn installs_data_and_refuses_overwrite() {
        let t = Temp::new("install");
        png(&t.source().join("wallpapers/with spaces.png"));
        fs::write(t.source().join("LICENSE"), "license").unwrap();
        assert_eq!(install(&t.home(), &t.source()).unwrap(), "mine");
        assert!(crate::Theme::load(&t.home(), "mine").is_ok());
        assert!(t.home().join("themes/mine/LICENSE").is_file());
        let dir = wallpaper_dir(&t.home(), "mine").unwrap();
        assert_eq!(images(&dir).unwrap(), ["with spaces.png"]);
        assert_eq!(
            decode(&dir.join("with spaces.png")).unwrap().dimensions(),
            (2, 2)
        );
        assert!(install(&t.home(), &t.source()).is_err());
        assert!(t.source().join("theme.toml").is_file());
    }
    #[test]
    fn bad_images_and_palettes_leave_nothing_installed() {
        let t = Temp::new("bad");
        fs::write(t.source().join("wallpapers/bad.jpg"), "not jpeg").unwrap();
        assert!(install(&t.home(), &t.source()).is_err());
        assert_eq!(fs::read_dir(t.home().join("themes")).unwrap().count(), 0);
        fs::write(t.source().join("theme.toml"), "broken").unwrap();
        assert!(install(&t.home(), &t.source()).is_err());
    }
    #[test]
    fn accelerated_cover_preserves_opaque_colors_and_transparency() {
        let gradient = image::RgbaImage::from_fn(32, 16, |x, y| {
            image::Rgba([(x * 7) as u8, (y * 13) as u8, 40, 255])
        });
        let fitted = cover(&gradient, 16, 8).unwrap();
        let reference =
            image::imageops::resize(&gradient, 16, 8, image::imageops::FilterType::Triangle);
        for (a, b) in fitted.as_raw().iter().zip(reference.as_raw()) {
            assert!(a.abs_diff(*b) <= 2);
        }
        let transparent = image::RgbaImage::from_pixel(4, 4, image::Rgba([120, 60, 30, 128]));
        let fitted = cover(&transparent, 8, 8).unwrap();
        for pixel in fitted.pixels() {
            assert_eq!(pixel[3], 128);
            assert!(pixel[0].abs_diff(120) <= 2 && pixel[1].abs_diff(60) <= 2);
        }
    }
    #[test]
    fn cover_handles_tiny_images_and_extreme_aspect_ratios() {
        for (width, height) in [(1, 1), (2, 2), (1, 16384), (16384, 1)] {
            let pixels = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
            let fitted = cover(&pixels, 640, 360).unwrap();
            assert_eq!(fitted.dimensions(), (640, 360));
            assert_eq!(fitted.get_pixel(320, 180).0, [255, 0, 0, 255]);
        }
        assert!(cover(&image::RgbaImage::new(1, 1), 0, 100).is_err());
        assert!(cover(&image::RgbaImage::new(1, 1), 16384, 16384).is_err());
    }
    #[test]
    fn no_images_and_bounded_names() {
        let t = Temp::new("empty");
        fs::remove_dir(t.source().join("wallpapers")).unwrap();
        install(&t.home(), &t.source()).unwrap();
        assert!(
            images(&wallpaper_dir(&t.home(), "mine").unwrap())
                .unwrap()
                .is_empty()
        );
        for name in ["../x", "C:foo", "x\\foo", ".", "..", "x\ny", "x."] {
            assert!(!plain_name(name));
        }
        assert!(plain_name("Place Du Carrousel Paris 1900.jpg"));
        assert!(wallpaper_dir(&t.home(), "../bad").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_linked_directories_and_manifest() {
        use std::os::unix::fs::symlink;
        let t = Temp::new("links");
        fs::remove_file(t.source().join("theme.toml")).unwrap();
        fs::write(t.0.join("palette"), crate::DEFAULT).unwrap();
        symlink(t.0.join("palette"), t.source().join("theme.toml")).unwrap();
        assert!(install(&t.home(), &t.source()).is_err());
        fs::create_dir_all(t.home().join("themes")).unwrap();
        symlink(t.source(), t.home().join("themes/mine")).unwrap();
        assert!(wallpaper_dir(&t.home(), "mine").is_err());
    }
}
