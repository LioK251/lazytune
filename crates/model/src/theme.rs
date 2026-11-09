use serde::{Deserialize, Serialize};
use std::collections::HashSet;
pub const MAX_THEME_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub background: String,
    pub surface: String,
    pub text: String,
    pub muted: String,
    pub accent: String,
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            background: "#171717".into(),
            surface: "#222222".into(),
            text: "#DDDDDD".into(),
            muted: "#A3A3A3".into(),
            accent: "#DDDDDD".into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    Comfortable,
    Compact,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Block {
    Artwork,
    Queue,
    Lyrics,
    Details,
}
impl Block {
    pub fn label(self) -> &'static str {
        match self {
            Self::Artwork => "Artwork",
            Self::Queue => "Queue",
            Self::Lyrics => "Lyrics",
            Self::Details => "Track information",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub main: Vec<Block>,
    pub side: Vec<Block>,
    pub home: Vec<String>,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            main: vec![Block::Artwork, Block::Queue],
            side: vec![Block::Lyrics],
            home: vec!["recommendations".into(), "library".into()],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Wallpaper {
    pub format: String,
    pub data: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThemePack {
    pub version: u32,
    pub name: String,
    pub palette: Palette,
    pub density: Density,
    pub dim: f64,
    pub blur: u8,
    pub artwork_colors: bool,
    #[serde(default)]
    pub song_colors: bool,
    #[serde(default)]
    pub glassy_player: bool,
    pub layout: Layout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallpaper: Option<Wallpaper>,
}
impl Default for ThemePack {
    fn default() -> Self {
        Self {
            version: 1,
            name: "Monochrome".into(),
            palette: Palette::default(),
            density: Density::Comfortable,
            dim: 0.68,
            blur: 24,
            artwork_colors: false,
            song_colors: false,
            glassy_player: false,
            layout: Layout::default(),
            wallpaper: None,
        }
    }
}
fn color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c.as_bytes()[1..].iter().all(|b| b.is_ascii_hexdigit())
}
fn luminance(c: &str) -> f64 {
    let channel = |i| {
        let n = u8::from_str_radix(&c[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
        if n <= 0.04045 {
            n / 12.92
        } else {
            ((n + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
}
pub fn contrast(a: &str, b: &str) -> f64 {
    if !color(a) || !color(b) {
        return 0.0;
    }
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
impl ThemePack {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("This theme version is not supported".into());
        }
        if self.name.trim().is_empty()
            || self.name.len() > 80
            || self.name.chars().any(char::is_control)
        {
            return Err("Theme names must contain 1–80 printable bytes".into());
        }
        for c in [
            &self.palette.background,
            &self.palette.surface,
            &self.palette.text,
            &self.palette.muted,
            &self.palette.accent,
        ] {
            if !color(c) {
                return Err("Theme colors must use #RRGGBB".into());
            }
        }
        for bg in [&self.palette.background, &self.palette.surface] {
            for fg in [&self.palette.text, &self.palette.muted] {
                if contrast(bg, fg) < 4.5 {
                    return Err("Text and background colors need at least 4.5:1 contrast".into());
                }
            }
        }
        if contrast(&self.palette.accent, &self.palette.background) < 3.0 {
            return Err("Accent needs at least 3:1 contrast".into());
        }
        if !self.dim.is_finite() || !(0.0..=1.0).contains(&self.dim) || self.blur > 40 {
            return Err("Background settings are out of range".into());
        }
        let blocks: Vec<_> = self.layout.main.iter().chain(&self.layout.side).collect();
        if blocks.len() > 4 || blocks.iter().collect::<HashSet<_>>().len() != blocks.len() {
            return Err("Each block may appear only once".into());
        }
        let home = &self.layout.home;
        if home.len() > 2
            || home
                .iter()
                .any(|s| !["recommendations", "library"].contains(&s.as_str()))
            || home.iter().collect::<HashSet<_>>().len() != home.len()
        {
            return Err("Invalid Home sections".into());
        }
        if let Some(w) = &self.wallpaper {
            if !["png", "jpeg"].contains(&w.format.as_str())
                || w.data.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4
            {
                return Err("Wallpaper must be a PNG/JPEG below 10 MiB".into());
            }
        }
        Ok(())
    }
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() > MAX_THEME_BYTES {
            return Err("Theme files must be smaller than 16 MiB".into());
        }
        let pack: Self = serde_json::from_slice(data)
            .map_err(|_| "Invalid theme file or unsupported fields".to_string())?;
        pack.validate()?;
        Ok(pack)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_roundtrip() {
        let t = ThemePack::default();
        assert_eq!(
            ThemePack::parse(&serde_json::to_vec(&t).unwrap()).unwrap(),
            t
        );
    }
    #[test]
    fn rejects_code_duplicates_and_bad_contrast() {
        let mut value = serde_json::to_value(ThemePack::default()).unwrap();
        value["script"] = serde_json::json!("alert(1)");
        assert!(ThemePack::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        let mut t = ThemePack::default();
        t.palette.accent = "url(file:///secret)".into();
        assert!(t.validate().is_err());
        let mut t = ThemePack::default();
        t.layout.main.push(Block::Queue);
        assert!(t.validate().is_err());
        let mut t = ThemePack::default();
        t.palette.text = t.palette.background.clone();
        assert!(t.validate().is_err());
        assert!(ThemePack::parse(b"{\"version\":1,\"version\":1}").is_err());
    }
    #[test]
    fn hostile_theme_corpus_cannot_escape_validation() {
        assert!(ThemePack::parse(&vec![b' '; MAX_THEME_BYTES + 1]).is_err());
        assert!(
            ThemePack::parse(format!("{}0{}", "[".repeat(200), "]".repeat(200)).as_bytes())
                .is_err()
        );
        let bytes = serde_json::to_vec(&ThemePack::default()).unwrap();
        for i in 0..bytes.len() {
            let mut mutation = bytes.clone();
            mutation[i] ^= 0xff;
            assert!(ThemePack::parse(&mutation).is_err());
        }
        for field in [
            "script",
            "css",
            "url",
            "path",
            "plugins",
            "lastfm",
            "listenbrainz",
            "__proto__",
        ] {
            let mut value = serde_json::to_value(ThemePack::default()).unwrap();
            value[field] = serde_json::json!("untrusted");
            assert!(ThemePack::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
}
