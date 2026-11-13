mod catalog;
mod listening;
mod theme;
pub use catalog::*;
pub use listening::*;
use serde::{Deserialize, Serialize};
pub use theme::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub artist_id: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub artwork: String,
    #[serde(default)]
    pub duration: f64,
}

pub fn video_id(value: &str) -> bool {
    value.len() == 11
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub fn artwork_url(value: &str) -> bool {
    [
        "https://i.ytimg.com/",
        "https://i9.ytimg.com/",
        "https://lh3.googleusercontent.com/",
        "https://yt3.googleusercontent.com/",
        "https://yt3.ggpht.com/",
        "https://music.youtube.com/image/mixart?",
        "https://www.gstatic.com/youtube/media/ytm/images/",
    ]
    .iter()
    .any(|p| value.starts_with(p))
        && value.len() <= 2048
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaybackSnapshot {
    pub track: Track,
    pub playing: bool,
    #[serde(default)]
    pub play_requested: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f64,
    pub muted: bool,
    #[serde(default)]
    pub seekable: bool,
    pub liked: bool,
    pub ad: bool,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub signed_in: bool,
    #[serde(default)]
    pub queue: Vec<Track>,
    #[serde(default)]
    pub error: Option<String>,
}
impl PlaybackSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if !self.track.id.is_empty() && !video_id(&self.track.id) {
            return Err("Invalid track identifier".into());
        }
        if self.track.title.len() > 1024
            || self.track.artist.len() > 1024
            || self.track.album.len() > 1024
            || self.queue.len() > 500
        {
            return Err("Player data exceeds limits".into());
        }
        if !self.track.artwork.is_empty() && !artwork_url(&self.track.artwork) {
            return Err("Invalid artwork origin".into());
        }
        for track in std::iter::once(&self.track).chain(self.queue.iter()) {
            if !track.artist_id.is_empty() {
                if !track.artist_id.starts_with("UC") {
                    return Err("Invalid artist identifier".into());
                }
                CatalogRequest::Browse(track.artist_id.clone()).validate()?;
            }
        }
        if ![self.position, self.duration, self.volume]
            .iter()
            .all(|n| n.is_finite() && *n >= 0.0)
            || self.volume > 1.0
            || self.duration > 604800.0
            || self.position > 604800.0
        {
            return Err("Invalid playback values".into());
        }
        for t in &self.queue {
            if !video_id(&t.id)
                || t.title.len() > 1024
                || t.artist.len() > 1024
                || (!t.artwork.is_empty() && !artwork_url(&t.artwork))
            {
                return Err("Invalid queue".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PlayerCommand {
    Play(String),
    PlayCollection(String),
    Toggle,
    Pause,
    Resume,
    Next,
    Previous,
    Seek { position: f64, track_id: String },
    Follow { channel_id: String, follow: bool },
    Volume(f64),
    Mute,
    Like(bool),
    Shuffle,
    Repeat,
    Enqueue(String),
    Remove(usize),
    Move { from: usize, to: usize },
}
impl PlayerCommand {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::PlayCollection(id) => {
                if !(id.starts_with("VL") && id.len() > 2 || id.starts_with("MPRE") && id.len() > 4)
                {
                    return Err("Invalid playlist or album identifier".into());
                }
                CatalogRequest::Browse(id.clone()).validate()
            }
            Self::Play(id) | Self::Enqueue(id) if !video_id(id) => {
                Err("Invalid video identifier".into())
            }
            Self::Seek { position, track_id }
                if !position.is_finite()
                    || !(0.0..=604800.0).contains(position)
                    || !video_id(track_id) =>
            {
                Err("Invalid seek position".into())
            }
            Self::Follow { channel_id, .. } => {
                if !channel_id.starts_with("UC") {
                    return Err("Invalid artist identifier".into());
                }
                CatalogRequest::Browse(channel_id.clone()).validate()
            }
            Self::Volume(n) if !n.is_finite() || !(0.0..=1.0).contains(n) => {
                Err("Invalid volume".into())
            }
            Self::Remove(n) if *n >= 500 => Err("Invalid queue index".into()),
            Self::Move { from, to } if *from >= 500 || *to >= 500 => {
                Err("Invalid queue index".into())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PluginSettings {
    pub discord: bool,
    pub adblock: bool,
    pub sponsorblock: bool,
    pub lyrics: bool,
    pub lastfm: bool,
    pub listenbrainz: bool,
    pub sponsor_categories: Vec<String>,
    pub lyrics_offset: f64,
}
impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            discord: true,
            adblock: true,
            sponsorblock: false,
            lyrics: true,
            lastfm: false,
            listenbrainz: false,
            sponsor_categories: vec!["sponsor".into(), "music_offtopic".into()],
            lyrics_offset: 0.0,
        }
    }
}
impl PluginSettings {
    pub fn validate(&self) -> Result<(), String> {
        let allowed = [
            "sponsor",
            "music_offtopic",
            "intro",
            "outro",
            "selfpromo",
            "interaction",
            "preview",
        ];
        if self.sponsor_categories.len() > allowed.len()
            || self
                .sponsor_categories
                .iter()
                .any(|s| !allowed.contains(&s.as_str()))
            || !self.lyrics_offset.is_finite()
            || self.lyrics_offset.abs() > 30.0
        {
            return Err("Invalid integration settings".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub version: u32,
    pub close_to_tray: bool,
    pub sidebar_collapsed: bool,
    pub auto_video: bool,
    pub volume: Option<f64>,
    pub plugins: PluginSettings,
    pub theme: ThemePack,
    pub window_size: [u32; 2],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            close_to_tray: true,
            sidebar_collapsed: false,
            auto_video: false,
            volume: None,
            plugins: PluginSettings::default(),
            theme: ThemePack::default(),
            window_size: [1280, 820],
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PluginStatus {
    pub id: String,
    pub state: String,
    pub detail: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bootstrap {
    pub platform: String,
    pub settings: Settings,
    pub playback: PlaybackSnapshot,
    pub warning: Option<String>,
    pub version: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub release_url: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SurfaceBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl SurfaceBounds {
    pub fn validate(&self) -> Result<(), String> {
        if [self.x, self.y, self.width, self.height]
            .iter()
            .any(|n| !n.is_finite() || !(0.0..=16384.0).contains(n))
            || self.width < 1.0
            || self.height < 1.0
        {
            return Err("Invalid player surface".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LyricLine {
    pub time: f64,
    pub text: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub plain: String,
    pub source: String,
}
pub fn active_lyric(lines: &[LyricLine], position: f64) -> Option<usize> {
    if !position.is_finite() {
        return None;
    }
    lines.iter().rposition(|line| line.time <= position)
}

pub fn parse_lrc(input: &str) -> Vec<LyricLine> {
    let mut result = Vec::new();
    for line in input.lines().take(10000) {
        let mut rest = line;
        let mut times = Vec::new();
        while let Some(s) = rest.strip_prefix('[') {
            let Some((stamp, tail)) = s.split_once(']') else {
                break;
            };
            if let Some((m, s)) = stamp.split_once(':') {
                if let (Ok(m), Ok(s)) = (m.parse::<u32>(), s.parse::<f64>()) {
                    if m < 10000 && s.is_finite() && (0.0..60.0).contains(&s) {
                        times.push(m as f64 * 60.0 + s);
                    }
                }
            }
            rest = tail;
        }
        for time in times {
            result.push(LyricLine {
                time,
                text: rest.chars().take(1000).collect(),
            });
        }
    }
    result.sort_by(|a, b| a.time.total_cmp(&b.time));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_profiles_enable_requested_defaults_and_preserve_saved_opt_outs() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(settings.theme.song_colors);
        assert!(settings.theme.artwork_colors);
        assert!(settings.theme.glassy_player);
        assert!(settings.plugins.discord);
        assert!(settings.plugins.adblock);
        settings.theme.song_colors = false;
        settings.theme.artwork_colors = false;
        settings.theme.glassy_player = false;
        settings.plugins.discord = false;
        settings.plugins.adblock = false;
        let saved = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&saved).unwrap(), settings);
    }
    #[test]
    fn validates_commands_and_origins() {
        assert!(PlayerCommand::Play("dQw4w9WgXcQ".into()).validate().is_ok());
        assert!(PlayerCommand::PlayCollection("VLPLtest".into())
            .validate()
            .is_ok());
        assert!(PlayerCommand::PlayCollection("MPREalbum".into())
            .validate()
            .is_ok());
        for id in ["VL", "UCartist", "VLbad/endpoint", "https://evil.test"] {
            assert!(PlayerCommand::PlayCollection(id.into()).validate().is_err());
        }
        assert!(PlayerCommand::Seek {
            position: f64::NAN,
            track_id: "dQw4w9WgXcQ".into()
        }
        .validate()
        .is_err());
        assert!(PlayerCommand::Play("';alert(1)//".into())
            .validate()
            .is_err());
        assert!(!artwork_url("https://i.ytimg.com.evil.com/a"));
        assert!(SurfaceBounds {
            x: f64::NAN,
            y: 0.0,
            width: 800.0,
            height: 600.0
        }
        .validate()
        .is_err());
        assert!(SurfaceBounds {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0
        }
        .validate()
        .is_ok());
    }
    #[test]
    fn timed_lyrics_sort_repeat_and_ignore_metadata() {
        let lines = parse_lrc("[ar:Test]\n[00:20.5]Later\n[00:01.00][00:05.00]Again\n[00:NaN]bad");
        assert_eq!(
            lines.iter().map(|l| l.time).collect::<Vec<_>>(),
            vec![1.0, 5.0, 20.5]
        );
        assert_eq!(lines[0].text, "Again");
        assert_eq!(active_lyric(&lines, 0.0), None);
        assert_eq!(active_lyric(&lines, 5.0), Some(1));
        assert_eq!(active_lyric(&lines, 60.0), Some(2));
    }
}
