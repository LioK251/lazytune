use crate::{
    app::{control, AppState},
    storage,
};
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use lazytune_model::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::atomic::Ordering,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

const LASTFM: &str = "https://ws.audioscrobbler.com/2.0/";
const DISCORD_ID: &str = match option_env!("LAZYTUNE_DISCORD_ID") {
    Some(s) => s,
    None => "1569240583053123675",
};
fn key() -> Result<&'static str, String> {
    option_env!("LAZYTUNE_LASTFM_KEY")
        .filter(|s| !s.is_empty())
        .ok_or("This build needs lazytune's Last.fm application key".into())
}
fn signature(values: &BTreeMap<String, String>) -> Result<String, String> {
    let secret = option_env!("LAZYTUNE_LASTFM_SECRET")
        .filter(|s| !s.is_empty())
        .ok_or("This build needs lazytune's Last.fm shared secret")?;
    let mut s = String::new();
    for (k, v) in values {
        if k != "format" {
            s.push_str(k);
            s.push_str(v);
        }
    }
    s.push_str(secret);
    Ok(format!("{:x}", md5::compute(s)))
}
pub async fn json_response(mut response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Service returned HTTP {}", status.as_u16()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Service connection interrupted")?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("Service response exceeds limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Service returned an invalid response".into())
}
async fn lastfm(
    app: &AppHandle,
    method: &str,
    values: impl IntoIterator<Item = (String, String)>,
) -> Result<Value, String> {
    let mut body: BTreeMap<String, String> = values.into_iter().collect();
    body.insert("method".into(), method.into());
    body.insert("api_key".into(), key()?.into());
    body.insert("api_sig".into(), signature(&body)?);
    body.insert("format".into(), "json".into());
    let r = app
        .state::<AppState>()
        .client
        .post(LASTFM)
        .form(&body)
        .send()
        .await
        .map_err(|_| "Could not reach Last.fm")?;
    let result = json_response(r).await?;
    if let Some(code) = result.get("error").and_then(Value::as_u64) {
        return Err(format!(
            "Last.fm error {code}; reconnect your account if authorization expired"
        ));
    }
    Ok(result)
}
pub async fn lastfm_connect(app: &AppHandle) -> Result<String, String> {
    let data = lastfm(app, "auth.getToken", []).await?;
    let token = data["token"]
        .as_str()
        .filter(|s| s.len() <= 128)
        .ok_or("Last.fm did not return an authorization token")?;
    let mut url = url::Url::parse("https://www.last.fm/api/auth/").unwrap();
    url.query_pairs_mut()
        .append_pair("api_key", key()?)
        .append_pair("token", token);
    open::that(url.as_str()).map_err(|_| "Could not open the browser")?;
    Ok(token.into())
}
pub async fn lastfm_finish(app: &AppHandle, token: &str) -> Result<(), String> {
    if token.len() > 128 || !token.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("Invalid authorization token".into());
    }
    let data = lastfm(app, "auth.getSession", [("token".into(), token.into())]).await?;
    let session = data
        .pointer("/session/key")
        .and_then(Value::as_str)
        .ok_or("Approve access in your browser, then try again")?
        .to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        storage::secret("lastfm")?
            .set_password(&session)
            .map_err(|_| "Could not save the session in the OS credential store".to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    app.state::<AppState>()
        .status("lastfm", "connected", "Account connected");
    Ok(())
}
pub async fn lyrics(app: &AppHandle) -> Result<Lyrics, String> {
    let s = app.state::<AppState>();
    if !s.settings.lock().unwrap().plugins.lyrics {
        return Err("Enable synchronized lyrics in Integrations".into());
    }
    let p = s.playback.lock().unwrap().clone();
    if p.track.id.is_empty() {
        return Ok(Lyrics::default());
    }
    let r = s
        .client
        .get("https://lrclib.net/api/get")
        .query(&[
            ("track_name", p.track.title.clone()),
            ("artist_name", p.track.artist.clone()),
            ("album_name", p.track.album.clone()),
            ("duration", p.duration.round().to_string()),
        ])
        .send()
        .await
        .map_err(|_| "Could not reach LRCLIB")?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Lyrics {
            source: "No lyrics found for this track".into(),
            ..Lyrics::default()
        });
    }
    let data = json_response(r).await?;
    if s.playback.lock().unwrap().track.id != p.track.id {
        return Err("Track changed; request lyrics again".into());
    }
    let synced = data["syncedLyrics"].as_str().unwrap_or_default();
    let plain = data["plainLyrics"]
        .as_str()
        .unwrap_or_default()
        .chars()
        .take(100000)
        .collect();
    Ok(Lyrics {
        lines: parse_lrc(synced),
        plain,
        source: "LRCLIB".into(),
    })
}
pub async fn artwork_colors(app: &AppHandle) -> Result<Vec<String>, String> {
    let s = app.state::<AppState>();
    let artwork = s.playback.lock().unwrap().track.artwork.clone();
    if !artwork_url(&artwork) {
        return Ok(vec![]);
    }
    let mut r = s
        .client
        .get(artwork)
        .header("Accept", "image/jpeg,image/png")
        .send()
        .await
        .map_err(|_| "Could not load artwork")?
        .error_for_status()
        .map_err(|_| "Artwork is unavailable")?;
    let mut bytes = Vec::new();
    while let Some(chunk) = r
        .chunk()
        .await
        .map_err(|_| "Artwork download interrupted")?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("Artwork exceeds limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    tauri::async_runtime::spawn_blocking(move || storage::artwork_colors(&bytes))
        .await
        .map_err(|e| e.to_string())?
}
async fn segments(
    app: &AppHandle,
    p: &PlaybackSnapshot,
    categories: &[String],
) -> Result<Vec<Segment>, String> {
    let digest = format!("{:x}", Sha256::digest(p.track.id.as_bytes()));
    let r = app
        .state::<AppState>()
        .client
        .get(format!(
            "https://sponsor.ajay.app/api/skipSegments/{}",
            &digest[..4]
        ))
        .query(&[("categories", serde_json::to_string(categories).unwrap())])
        .send()
        .await
        .map_err(|_| "Could not reach SponsorBlock")?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(vec![]);
    }
    let data = json_response(r).await?;
    let mut result = Vec::new();
    if let Some(rows) = data.as_array() {
        for row in rows.iter().take(1000) {
            if row["videoID"].as_str() != Some(&p.track.id) {
                continue;
            }
            if let Some(segs) = row["segments"].as_array() {
                for seg in segs.iter().take(1000) {
                    let category = seg["category"].as_str().unwrap_or_default();
                    if !categories.iter().any(|c| c == category) {
                        continue;
                    }
                    if let (Some(start), Some(end)) = (
                        seg.pointer("/segment/0").and_then(Value::as_f64),
                        seg.pointer("/segment/1").and_then(Value::as_f64),
                    ) {
                        result.push(Segment {
                            start,
                            end,
                            category: category.into(),
                        });
                    }
                }
            }
        }
    }
    Ok(clean_segments(result, p.duration))
}
#[derive(Clone, Serialize, Deserialize)]
struct PendingListen {
    provider: String,
    account: String,
    track: Track,
    started: i64,
}
fn credential(name: &str) -> Option<String> {
    storage::secret(name).ok()?.get_password().ok()
}
fn fingerprint(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
async fn submit(
    app: &AppHandle,
    listen: &PendingListen,
    token: &str,
    now: bool,
) -> Result<(), String> {
    if listen.provider == "lastfm" {
        let mut body = vec![
            ("sk".into(), token.into()),
            ("track".into(), listen.track.title.clone()),
            ("artist".into(), listen.track.artist.clone()),
            ("duration".into(), listen.track.duration.round().to_string()),
        ];
        if !listen.track.album.is_empty() {
            body.push(("album".into(), listen.track.album.clone()));
        }
        if !now {
            body.push(("timestamp".into(), listen.started.to_string()));
        }
        lastfm(
            app,
            if now {
                "track.updateNowPlaying"
            } else {
                "track.scrobble"
            },
            body,
        )
        .await?;
    } else {
        let mut payload = json!({"track_metadata":{"artist_name":listen.track.artist,"track_name":listen.track.title,"release_name":listen.track.album,"additional_info":{"submission_client":"lazytune","submission_client_version":"0.1.0","duration_ms":(listen.track.duration*1000.0)as u64}}});
        if !now {
            payload["listened_at"] = json!(listen.started);
        }
        let data = json!({"listen_type":if now{"playing_now"}else{"single"},"payload":[payload]});
        let r = app
            .state::<AppState>()
            .client
            .post("https://api.listenbrainz.org/1/submit-listens")
            .header("Authorization", format!("Token {token}"))
            .json(&data)
            .send()
            .await
            .map_err(|_| "Could not reach ListenBrainz")?;
        json_response(r).await?;
    }
    Ok(())
}
pub async fn run(app: AppHandle) {
    let s = app.state::<AppState>();
    let outpath = s.directory.join("scrobbles.json");
    let mut outbox: Vec<PendingListen> = storage::read_limited(&outpath, 2 * 1024 * 1024)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    outbox.truncate(1000);
    let start = Instant::now();
    let mut listening = Listening::default();
    let mut last_id = String::new();
    let mut now_sent = String::new();
    let mut now_retry = BTreeMap::new();
    let mut cached_segments = Vec::new();
    let mut previous_config = PluginSettings::default();
    let mut discord = DiscordIpcClient::new(DISCORD_ID);
    let mut connected = false;
    let mut discord_key = String::new();
    let mut reconnect = Instant::now();
    let mut retry = Instant::now();
    let mut retry_delay = 5u64;
    let mut skipped_until = 0.0;
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let config = s.settings.lock().unwrap().plugins.clone();
        let p = s.playback.lock().unwrap().clone();
        let generation = s.generation.load(Ordering::SeqCst);
        let unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if (config.lastfm, config.listenbrainz)
            != (previous_config.lastfm, previous_config.listenbrainz)
        {
            listening.reset();
            now_sent.clear();
        }
        if config.sponsor_categories != previous_config.sponsor_categories
            || config.sponsorblock != previous_config.sponsorblock
        {
            last_id.clear();
        }
        previous_config = config.clone();
        s.status(
            "adblock",
            if config.adblock {
                "enabled"
            } else {
                "disabled"
            },
            "Built-in YouTube player filter",
        );
        s.status(
            "lyrics",
            if config.lyrics { "enabled" } else { "disabled" },
            "LRCLIB · fetched when you open lyrics",
        );
        if !config.discord {
            if connected {
                let _ = discord.clear_activity();
                let _ = discord.close();
                connected = false;
                discord_key.clear();
            }
            s.status("discord", "disabled", "Your listening activity is private");
        } else {
            if !connected && Instant::now() >= reconnect {
                connected = discord.connect().is_ok();
                reconnect = Instant::now() + Duration::from_secs(15);
            }
            s.status(
                "discord",
                if connected { "connected" } else { "waiting" },
                if connected {
                    "Connected to Discord"
                } else {
                    "Waiting for the Discord desktop app"
                },
            );
            let stamp = format!(
                "{}:{}:{}",
                p.track.id,
                p.playing,
                (p.position / 15.0) as i64
            );
            if connected && stamp != discord_key {
                let result = if p.playing && !p.track.id.is_empty() && !p.ad {
                    let title: String = p.track.title.chars().take(120).collect();
                    let artist: String = p.track.artist.chars().take(120).collect();
                    let url = format!("https://music.youtube.com/watch?v={}", p.track.id);
                    let mut activity = activity::Activity::new()
                        .name("Music")
                        .activity_type(activity::ActivityType::Listening)
                        .details(if title.chars().count() < 2 {
                            "Music"
                        } else {
                            &title
                        })
                        .state(if artist.chars().count() < 2 {
                            "YouTube Music"
                        } else {
                            &artist
                        })
                        .timestamps(
                            activity::Timestamps::new()
                                .start(unix - p.position as i64)
                                .end(unix + (p.duration - p.position).max(0.0) as i64),
                        )
                        .buttons(vec![activity::Button::new("Listen", &url)]);
                    if !p.track.artwork.is_empty() {
                        activity =
                            activity.assets(activity::Assets::new().large_image(&p.track.artwork));
                    }
                    discord.set_activity(activity)
                } else {
                    discord.clear_activity()
                };
                if result.is_err() {
                    connected = false;
                    let _ = discord.close();
                }
                discord_key = stamp;
            }
        }
        if p.track.id != last_id {
            last_id = p.track.id.clone();
            cached_segments.clear();
            skipped_until = 0.0;
            if config.sponsorblock && !p.track.id.is_empty() {
                let result = tokio::select! {r=segments(&app,&p,&config.sponsor_categories)=>Some(r),_=s.changed.notified()=>None};
                if let Some(result) = result {
                    match result {
                        Ok(v) => {
                            cached_segments = v;
                            s.status("sponsorblock", "enabled", "Segments loaded");
                        }
                        Err(e) => s.status("sponsorblock", "error", &e),
                    }
                }
            }
        }
        if !config.sponsorblock {
            s.status("sponsorblock", "disabled", "No segments are skipped");
        }
        if s.generation.load(Ordering::SeqCst) != generation {
            continue;
        }
        let current = s.playback.lock().unwrap().clone();
        if current.track.id != p.track.id {
            continue;
        }
        let p = current;
        if config.sponsorblock && p.playing && p.seekable && !p.ad {
            if let Some(status) = s.statuses.lock().unwrap().get("sponsorblock") {
                if status.state == "undo" {
                    skipped_until = status.detail.parse().unwrap_or(0.0);
                }
            }
            for segment in &cached_segments {
                if p.position >= segment.start
                    && p.position < segment.end
                    && segment.end > skipped_until
                {
                    *s.undo.lock().unwrap() =
                        Some((p.track.id.clone(), segment.start, segment.end));
                    skipped_until = segment.end;
                    if control(
                        &app,
                        PlayerCommand::Seek {
                            position: segment.end,
                            track_id: p.track.id.clone(),
                        },
                    )
                    .await
                    .is_ok()
                    {
                        let _ = app.emit_to(
                            "main",
                            "notice",
                            "Skipped a SponsorBlock segment · Undo is available in the player",
                        );
                    }
                    break;
                }
            }
        }
        let eligible = listening.update(&p, start.elapsed().as_secs_f64(), unix);
        for (provider, enabled) in [
            ("lastfm", config.lastfm),
            ("listenbrainz", config.listenbrainz),
        ] {
            if !enabled {
                s.status(provider, "disabled", "Scrobbling is off");
                continue;
            }
            let Some(token) = credential(provider) else {
                s.status(provider, "setup", "Connect your account in Integrations");
                continue;
            };
            if p.track.id.is_empty() || p.ad {
                continue;
            }
            let mut track = p.track.clone();
            track.duration = p.duration;
            let listen = PendingListen {
                provider: provider.into(),
                account: fingerprint(&token),
                track,
                started: listening.started,
            };
            let now_key = format!("{}:{}:{}", provider, p.track.id, listening.started);
            if p.playing
                && !now_sent.contains(&now_key)
                && now_retry
                    .get(provider)
                    .is_none_or(|at| Instant::now() >= *at)
            {
                now_retry.insert(provider, Instant::now() + Duration::from_secs(30));
                let result = tokio::select! {r=submit(&app,&listen,&token,true)=>Some(r),_=s.changed.notified()=>None};
                if let Some(result) = result {
                    match result {
                        Ok(()) => {
                            now_sent.push_str(&now_key);
                            now_sent.push('|');
                            s.status(provider, "connected", "Now Playing updated");
                        }
                        Err(e) => s.status(provider, "error", &e),
                    }
                }
            }
            if eligible
                && s.generation.load(Ordering::SeqCst) == generation
                && !outbox.iter().any(|q| {
                    q.provider == provider
                        && q.started == listen.started
                        && q.track.id == listen.track.id
                })
            {
                if outbox.len() == 1000 {
                    outbox.remove(0);
                }
                outbox.push(listen);
                if storage::atomic_json(&outpath, &outbox).is_err() {
                    s.status(
                        provider,
                        "error",
                        "Could not save pending scrobble; check disk space",
                    );
                }
            }
        }
        if now_sent.len() > 4096 {
            now_sent.clear();
        }
        if Instant::now() >= retry {
            outbox.retain(|q| unix - q.started <= 7 * 86400);
            if let Some(index) = outbox.iter().position(|q| {
                if q.provider == "lastfm" {
                    config.lastfm
                } else {
                    config.listenbrainz
                }
            }) {
                let listen = outbox[index].clone();
                if let Some(token) = credential(&listen.provider) {
                    if fingerprint(&token) != listen.account {
                        outbox.remove(index);
                        let _ = storage::atomic_json(&outpath, &outbox);
                        continue;
                    }
                    let result = tokio::select! {r=submit(&app,&listen,&token,false)=>Some(r),_=s.changed.notified()=>None};
                    if let Some(result) = result {
                        match result {
                            Ok(()) => {
                                outbox.remove(index);
                                retry_delay = 5;
                                s.status(
                                    &listen.provider,
                                    "connected",
                                    "Listening history is up to date",
                                );
                                if storage::atomic_json(&outpath, &outbox).is_err() {
                                    s.status(
                                        &listen.provider,
                                        "error",
                                        "Could not persist scrobble receipt",
                                    );
                                }
                            }
                            Err(e) => {
                                retry_delay = (retry_delay * 2).min(300);
                                s.status(&listen.provider, "retrying", &e);
                            }
                        }
                    }
                }
            }
            retry = Instant::now() + Duration::from_secs(retry_delay);
        }
    }
}
