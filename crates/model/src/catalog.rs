use crate::{artwork_url, video_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CatalogRequest {
    Home,
    Library,
    Liked,
    Search(String),
    Browse(String),
    Continue { token: String, search: bool },
}
impl CatalogRequest {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Search(q) if q.trim().is_empty() || q.len() > 500 => {
                Err("Enter a search of up to 500 characters".into())
            }
            Self::Browse(id)
                if id.is_empty()
                    || id.len() > 256
                    || !id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)) =>
            {
                Err("Invalid browse identifier".into())
            }
            Self::Continue { token, .. } if token.is_empty() || token.len() > 16384 => {
                Err("Invalid continuation".into())
            }
            _ => Ok(()),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CatalogItem {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artwork: String,
    pub playable: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Shelf {
    pub title: String,
    pub items: Vec<CatalogItem>,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ArtistSubscription {
    pub channel_id: String,
    pub followed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CatalogPage {
    pub shelves: Vec<Shelf>,
    pub continuation: Option<String>,
    #[serde(default)]
    pub artwork: String,
    #[serde(default)]
    pub subscription: Option<ArtistSubscription>,
}
fn text(v: &Value) -> String {
    if let Some(s) = v.get("simpleText").and_then(Value::as_str) {
        return s.chars().take(1000).collect();
    }
    v.get("runs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("text").and_then(Value::as_str))
                .collect::<String>()
                .chars()
                .take(1000)
                .collect()
        })
        .unwrap_or_default()
}
fn find_string<'a>(v: &'a Value, key: &str, depth: u8) -> Option<&'a str> {
    if depth > 12 {
        return None;
    }
    if let Some(s) = v.get(key).and_then(Value::as_str) {
        return Some(s);
    }
    match v {
        Value::Object(m) => m.values().find_map(|v| find_string(v, key, depth + 1)),
        Value::Array(a) => a.iter().find_map(|v| find_string(v, key, depth + 1)),
        _ => None,
    }
}
fn artwork(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            if let Some(a) = m.get("thumbnails").and_then(Value::as_array) {
                return a
                    .iter()
                    .rev()
                    .filter_map(|x| x.get("url").and_then(Value::as_str))
                    .find(|s| artwork_url(s))
                    .unwrap_or_default()
                    .to_string();
            }
            m.values()
                .map(artwork)
                .find(|s| !s.is_empty())
                .unwrap_or_default()
        }
        Value::Array(a) => a
            .iter()
            .map(artwork)
            .find(|s| !s.is_empty())
            .unwrap_or_default(),
        _ => String::new(),
    }
}
fn item(v: &Value) -> Option<CatalogItem> {
    let r = [
        "musicCardShelfRenderer",
        "musicResponsiveListItemRenderer",
        "musicTwoRowItemRenderer",
        "playlistPanelVideoRenderer",
    ]
    .iter()
    .find_map(|k| v.get(*k))?;
    let first = r.pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text");
    let title = text(r.get("title").or(first).unwrap_or(&Value::Null));
    if title.is_empty() {
        return None;
    }
    let endpoint = r.get("navigationEndpoint").or_else(|| r.get("onTap")).or_else(||r.pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint"));
    let vid = r
        .get("videoId")
        .and_then(Value::as_str)
        .or_else(|| {
            r.pointer("/playlistItemData/videoId")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            endpoint
                .and_then(|e| e.pointer("/watchEndpoint/videoId"))
                .and_then(Value::as_str)
        });
    let (id, playable) = if let Some(id) = vid.filter(|id| video_id(id)) {
        (id, true)
    } else {
        (
            endpoint
                .and_then(|e| e.pointer("/browseEndpoint/browseId"))
                .and_then(Value::as_str)?,
            false,
        )
    };
    let subtitle = text(
        r.get("subtitle")
            .or_else(|| r.get("longBylineText"))
            .or_else(|| r.pointer("/flexColumns/1/musicResponsiveListItemFlexColumnRenderer/text"))
            .unwrap_or(&Value::Null),
    );
    Some(CatalogItem {
        id: id.into(),
        title,
        subtitle,
        artwork: artwork(r),
        playable,
    })
}
pub fn parse_catalog(value: &Value) -> CatalogPage {
    fn walk(v: &Value, page: &mut CatalogPage, loose: &mut Vec<CatalogItem>, depth: u8) {
        if depth > 32
            || page.shelves.iter().map(|s| s.items.len()).sum::<usize>() + loose.len() >= 500
        {
            return;
        }
        if let Some(i) = item(v) {
            loose.push(i);
            if let Some(contents) = v.pointer("/musicCardShelfRenderer/contents") {
                walk(contents, page, loose, depth + 1);
            }
            return;
        }
        if page.continuation.is_none() {
            page.continuation = find_string(
                v.get("continuations").unwrap_or(&Value::Null),
                "continuation",
                0,
            )
            .map(str::to_string);
        }
        if let Some(token) = v
            .pointer("/continuationItemRenderer/continuationEndpoint/continuationCommand/token")
            .and_then(Value::as_str)
        {
            page.continuation = Some(token.into());
        }
        for key in [
            "musicCarouselShelfRenderer",
            "musicShelfRenderer",
            "musicPlaylistShelfRenderer",
        ] {
            if let Some(r) = v.get(key) {
                if !loose.is_empty() {
                    page.shelves.push(Shelf {
                        title: String::new(),
                        items: std::mem::take(loose),
                    });
                }
                let title = text(
                    r.get("title")
                        .or_else(|| {
                            r.pointer("/header/musicCarouselShelfBasicHeaderRenderer/title")
                        })
                        .unwrap_or(&Value::Null),
                );
                let mut items = Vec::new();
                if let Some(contents) = r.get("contents") {
                    walk(contents, page, &mut items, depth + 1);
                }
                if !items.is_empty() {
                    page.shelves.push(Shelf { title, items });
                }
                if let Some(c) = r.get("continuations") {
                    if page.continuation.is_none() {
                        page.continuation = find_string(c, "continuation", 0).map(str::to_string);
                    }
                }
                return;
            }
        }
        match v {
            Value::Object(m) => {
                for (k, v) in m {
                    if !["menu", "trackingParams", "background", "header"].contains(&k.as_str()) {
                        walk(v, page, loose, depth + 1);
                    }
                }
            }
            Value::Array(a) => {
                for v in a {
                    walk(v, page, loose, depth + 1);
                }
            }
            _ => {}
        }
    }
    let mut page = CatalogPage {
        artwork: artwork(value.get("background").unwrap_or(&Value::Null)),
        ..CatalogPage::default()
    };
    if page.artwork.is_empty() {
        page.artwork = artwork(value.get("header").unwrap_or(&Value::Null));
    }
    if let Some(header) = value.get("header").and_then(Value::as_object) {
        if let Some(button) = header
            .values()
            .find_map(|h| h.pointer("/subscriptionButton/subscribeButtonRenderer"))
        {
            if let Some(id) = button
                .get("channelId")
                .and_then(Value::as_str)
                .filter(|id| {
                    id.starts_with("UC") && CatalogRequest::Browse((*id).into()).validate().is_ok()
                })
            {
                page.subscription = Some(ArtistSubscription {
                    channel_id: id.into(),
                    followed: button
                        .get("subscribed")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                });
            }
        }
    }
    let mut loose = Vec::new();
    walk(value, &mut page, &mut loose, 0);
    if !loose.is_empty() {
        page.shelves.push(Shelf {
            title: String::new(),
            items: loose,
        });
    }
    let mut seen = std::collections::HashSet::new();
    for shelf in &mut page.shelves {
        shelf.items.retain(|i| seen.insert(i.id.clone()));
    }
    page.shelves.retain(|s| !s.items.is_empty());
    page
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_top_card_precedes_shelves_and_keeps_related_results() {
        let song = serde_json::json!({"musicResponsiveListItemRenderer": {
            "title": {"runs": [{"text": "цветы"}]},
            "playlistItemData": {"videoId": "_5nSnBpjTFY"}
        }});
        let input = serde_json::json!({"contents": [
            {"musicCardShelfRenderer": {
                "title": {"runs": [{"text": "цветы"}]},
                "subtitle": {"runs": [{"text": "тёмный принц"}]},
                "onTap": {"watchEndpoint": {"videoId": "_5nSnBpjTFY"}},
                "thumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                    {"url": "https://i.ytimg.com/vi/_5nSnBpjTFY/hqdefault.jpg"}
                ]}}},
                "contents": [{"musicTwoRowItemRenderer": {
                    "title": {"runs": [{"text": "Related song"}]},
                    "navigationEndpoint": {"watchEndpoint": {"videoId": "dQw4w9WgXcQ"}}
                }}]
            }},
            {"musicShelfRenderer": {"title": {"simpleText": "Songs"}, "contents": [song]}}
        ]});
        let page = parse_catalog(&input);
        let items: Vec<_> = page.shelves.iter().flat_map(|s| &s.items).collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "_5nSnBpjTFY");
        assert_eq!(items[0].title, "цветы");
        assert_eq!(items[0].subtitle, "тёмный принц");
        assert!(items[0].playable);
        assert!(!items[0].artwork.is_empty());
        assert_eq!(items[1].id, "dQw4w9WgXcQ");
    }
    #[test]
    fn generated_mix_and_liked_playlist_artwork_is_preserved() {
        for url in [
            "https://music.youtube.com/image/mixart?r=example",
            "https://www.gstatic.com/youtube/media/ytm/images/pbg/liked-songs-delhi-576.png",
        ] {
            let input = serde_json::json!({"musicTwoRowItemRenderer":{
                "title":{"runs":[{"text":"Playlist"}]},
                "navigationEndpoint":{"browseEndpoint":{"browseId":"VLplaylist"}},
                "thumbnailRenderer":{"musicThumbnailRenderer":{"thumbnail":{"thumbnails":[{"url":url}]}}}
            }});
            assert_eq!(parse_catalog(&input).shelves[0].items[0].artwork, url);
        }
        assert!(!artwork_url(
            "https://music.youtube.com.evil.test/image/mixart?r=x"
        ));
        assert!(!artwork_url(
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(!artwork_url("https://www.gstatic.com/unrelated/script.js"));
    }
    #[test]
    fn artist_header_provides_the_page_background() {
        let url = "https://yt3.googleusercontent.com/artist-banner";
        let input = serde_json::json!({"header":{"musicImmersiveHeaderRenderer":{
            "thumbnail":{"musicThumbnailRenderer":{"thumbnail":{"thumbnails":[{"url":url}]}}},
            "subscriptionButton":{"subscribeButtonRenderer":{"channelId":"UCofficial","subscribed":true}}
        }}});
        assert_eq!(parse_catalog(&input).artwork, url);
        let subscription = parse_catalog(&input).subscription.unwrap();
        assert_eq!(subscription.channel_id, "UCofficial");
        assert!(subscription.followed);
    }
    #[test]
    fn music_shelf_keeps_browse_and_play_targets_separate() {
        let input = serde_json::json!({"contents":{"musicShelfRenderer":{"title":{"runs":[{"text":"Songs"}]},"contents":[{"musicResponsiveListItemRenderer":{"flexColumns":[{"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Test"}]}}}],"playlistItemData":{"videoId":"dQw4w9WgXcQ"},"thumbnail":{"thumbnails":[{"url":"https://evil.test/image"}]}}}],"continuations":[{"nextContinuationData":{"continuation":"next"}}]}}});
        let p = parse_catalog(&input);
        assert_eq!(p.shelves[0].title, "Songs");
        assert!(p.shelves[0].items[0].playable);
        assert!(p.shelves[0].items[0].artwork.is_empty());
        assert_eq!(p.continuation.as_deref(), Some("next"));
    }
}
