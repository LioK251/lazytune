use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ImageFormat, ImageReader, Limits};
use lazytune_model::*;
use std::{
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::Path,
};

pub fn read_limited(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(max as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > max {
        return Err("File exceeds the allowed size".into());
    }
    Ok(data)
}
pub fn atomic_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid settings location")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut f = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut f, value).map_err(|e| e.to_string())?;
    f.flush()
        .and_then(|_| f.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    f.persist(path).map_err(|e| e.error.to_string())?;
    Ok(())
}
pub fn canonical_wallpaper(data: &[u8]) -> Result<Wallpaper, String> {
    if data.len() > MAX_IMAGE_BYTES {
        return Err("Wallpaper exceeds 10 MiB".into());
    }
    let format = image::guess_format(data).map_err(|_| "Unrecognized wallpaper image")?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg) {
        return Err("Choose a static PNG or JPEG".into());
    }
    if format == ImageFormat::Png {
        let mut at = 8usize;
        while at.checked_add(12).is_some_and(|end| end <= data.len()) {
            let len = u32::from_be_bytes(data[at..at + 4].try_into().map_err(|_| "Invalid PNG")?)
                as usize;
            if &data[at + 4..at + 8] == b"acTL" {
                return Err("Animated wallpapers are not supported".into());
            }
            at = at
                .checked_add(len)
                .and_then(|n| n.checked_add(12))
                .filter(|n| *n <= data.len())
                .ok_or("Invalid PNG chunk")?;
        }
    }
    let mut reader = ImageReader::with_format(Cursor::new(data), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "Image is damaged or exceeds 4096 × 4096 pixels")?;
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|_| "Could not normalize wallpaper")?;
    if output.get_ref().len() > MAX_IMAGE_BYTES {
        return Err("Normalized wallpaper exceeds 10 MiB; use a smaller image".into());
    }
    Ok(Wallpaper {
        format: "png".into(),
        data: STANDARD.encode(output.into_inner()),
    })
}
pub fn validate_theme(mut theme: ThemePack) -> Result<ThemePack, String> {
    theme.validate()?;
    if let Some(w) = theme.wallpaper.take() {
        let bytes = STANDARD
            .decode(&w.data)
            .map_err(|_| "Invalid wallpaper encoding")?;
        let actual = image::guess_format(&bytes).map_err(|_| "Invalid image")?;
        if (w.format == "png" && actual != ImageFormat::Png)
            || (w.format == "jpeg" && actual != ImageFormat::Jpeg)
        {
            return Err("Wallpaper format does not match its content".into());
        }
        theme.wallpaper = Some(canonical_wallpaper(&bytes)?);
    }
    if serde_json::to_vec(&theme).map_err(|e| e.to_string())?.len() > MAX_THEME_BYTES {
        return Err("Theme exceeds 16 MiB".into());
    }
    Ok(theme)
}
pub fn artwork_colors(data: &[u8]) -> Result<Vec<String>, String> {
    if data.len() > 2 * 1024 * 1024 {
        return Err("Artwork exceeds limit".into());
    }
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|_| "Invalid artwork")?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let pixels = reader
        .decode()
        .map_err(|_| "Unsupported artwork")?
        .thumbnail(64, 64)
        .to_rgb8();
    let mut bins = vec![(0u32, [0u32; 3]); 512];
    for p in pixels.pixels() {
        let key = ((p[0] as usize >> 5) << 6) | ((p[1] as usize >> 5) << 3) | (p[2] as usize >> 5);
        bins[key].0 += 1;
        for (i, c) in p.0.iter().enumerate() {
            bins[key].1[i] += *c as u32;
        }
    }
    bins.sort_by_key(|(n, rgb)| {
        std::cmp::Reverse(if *n == 0 {
            0
        } else {
            let max = *rgb.iter().max().unwrap() / n;
            let min = *rgb.iter().min().unwrap() / n;
            n * (32 + max - min)
        })
    });
    let mut colors: Vec<[u8; 3]> = Vec::new();
    for (n, sum) in bins.into_iter().filter(|(n, _)| *n > 0) {
        let rgb = sum.map(|c| (c / n) as u8);
        if colors.iter().all(|old| {
            old.iter()
                .zip(rgb)
                .map(|(a, b)| (*a as i32 - b as i32).pow(2))
                .sum::<i32>()
                > 3600
        }) {
            colors.push(rgb);
        }
        if colors.len() == 3 {
            break;
        }
    }
    if colors.is_empty() {
        return Err("Artwork has no pixels".into());
    }
    while colors.len() < 3 {
        colors.push(colors[0]);
    }
    let mut result: Vec<String> = colors
        .iter()
        .map(|rgb| {
            format!(
                "#{:02X}{:02X}{:02X}",
                rgb[0] as u16 * 28 / 100,
                rgb[1] as u16 * 28 / 100,
                rgb[2] as u16 * 28 / 100
            )
        })
        .collect();
    let c = colors[0].map(|n| 153 + n as u16 * 40 / 100);
    result.push(format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
    Ok(result)
}
pub fn load_settings(path: &Path) -> (Settings, Option<String>) {
    if !path.exists() {
        return (Settings::default(), None);
    }
    let loaded = (|| {
        let bytes = read_limited(path, MAX_THEME_BYTES + 65536)?;
        let mut s: Settings =
            serde_json::from_slice(&bytes).map_err(|_| "Settings file is damaged".to_string())?;
        if s.version != 1 {
            return Err("Settings version is not supported".into());
        }
        s.plugins.validate()?;
        if let Some(volume) = s.volume {
            PlayerCommand::Volume(volume).validate()?;
        }
        s.theme = validate_theme(s.theme)?;
        Ok::<_, String>(s)
    })();
    match loaded {
        Ok(s) => (s, None),
        Err(e) => (
            Settings::default(),
            Some(format!(
                "{e}. Defaults loaded; the original file is preserved until you save settings."
            )),
        ),
    }
}
pub fn secret(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("io.github.liok251.lazytune", name)
        .map_err(|_| "OS credential store is unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_volume_survives_reload_and_invalid_volume_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings {
            volume: Some(0.37),
            ..Settings::default()
        };
        atomic_json(&path, &settings).unwrap();
        let (loaded, warning) = load_settings(&path);
        assert_eq!(loaded.volume, Some(0.37));
        assert!(warning.is_none());
        settings.volume = Some(2.0);
        atomic_json(&path, &settings).unwrap();
        assert!(load_settings(&path).1.is_some());
    }
    #[test]
    fn malicious_or_huge_images_fail() {
        assert!(canonical_wallpaper(b"<svg onload='alert(1)'/>").is_err());
        assert!(canonical_wallpaper(&vec![0; MAX_IMAGE_BYTES + 1]).is_err());
    }
    #[test]
    fn artwork_palette_is_bounded_and_readable() {
        let mut data = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            12,
            12,
            image::Rgb([240, 30, 50]),
        ))
        .write_to(&mut data, ImageFormat::Png)
        .unwrap();
        let colors = artwork_colors(data.get_ref()).unwrap();
        assert_eq!(colors.len(), 4);
        for color in &colors[..3] {
            assert!(contrast(color, "#DDDDDD") >= 4.5);
        }
        assert!(contrast(&colors[3], "#222222") >= 4.5);
        assert!(artwork_colors(b"<svg/>").is_err());
    }
    #[test]
    fn wallpaper_is_normalized_and_roundtrips() {
        let mut data = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4, 4)
            .write_to(&mut data, ImageFormat::Png)
            .unwrap();
        let w = canonical_wallpaper(data.get_ref()).unwrap();
        let mut t = ThemePack::default();
        t.wallpaper = Some(w);
        assert!(validate_theme(t).is_ok());
    }
    #[test]
    fn invalid_import_cannot_touch_existing_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        atomic_json(&path, &Settings::default()).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(ThemePack::parse(b"{\"script\":\"evil\"}").is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
