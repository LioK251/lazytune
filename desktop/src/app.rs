use crate::{plugins, storage};
use lazytune_model::*;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, Webview};
use tokio::sync::{oneshot, Notify};

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub playback: Mutex<PlaybackSnapshot>,
    pub directory: PathBuf,
    pub warning: Mutex<Option<String>>,
    pub pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    pub next_id: AtomicU64,
    pub volume_revision: AtomicU64,
    pub generation: AtomicU64,
    pub changed: Notify,
    pub statuses: Mutex<HashMap<String, PluginStatus>>,
    pub client: reqwest::Client,
    pub undo: Mutex<Option<(String, f64, f64)>>,
    pub player_mode: Mutex<String>,
    pub preserve_settings: AtomicBool,
    pub tray_available: AtomicBool,
}
impl AppState {
    pub fn new(directory: PathBuf) -> Result<Self, String> {
        let (settings, warning) = storage::load_settings(&directory.join("settings.json"));
        let preserve_settings = AtomicBool::new(warning.is_some());
        let playback = PlaybackSnapshot {
            volume: settings.volume.unwrap_or(1.0),
            ..PlaybackSnapshot::default()
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("lazytune/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            settings: Mutex::new(settings),
            playback: Mutex::new(playback),
            directory,
            warning: Mutex::new(warning),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            volume_revision: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            changed: Notify::new(),
            statuses: Mutex::new(HashMap::new()),
            client,
            undo: Mutex::new(None),
            player_mode: Mutex::new("audio".into()),
            preserve_settings,
            tray_available: AtomicBool::new(false),
        })
    }
    pub fn status(&self, id: &str, state: &str, detail: &str) {
        if let Ok(mut statuses) = self.statuses.lock() {
            statuses.insert(
                id.into(),
                PluginStatus {
                    id: id.into(),
                    state: state.into(),
                    detail: detail.into(),
                },
            );
        }
    }
}
pub fn local(window: &Webview) -> Result<(), String> {
    if window.label() != "main" {
        return Err("This action is only available in lazytune".into());
    }
    let u = window.url().map_err(|e| e.to_string())?;
    let trusted = matches!(u.scheme(), "tauri")
        || (matches!(u.scheme(), "http" | "https") && u.host_str() == Some("tauri.localhost"));
    let development =
        cfg!(debug_assertions) && u.host_str() == Some("localhost") && u.port() == Some(1420);
    if !trusted && !development {
        return Err("Untrusted interface origin".into());
    }
    Ok(())
}
pub fn focus(app: &AppHandle) {
    if let Some(w) = app.get_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
pub fn configure_player(app: &AppHandle) {
    let s = app.state::<AppState>();
    let settings = s.settings.lock().unwrap();
    let (enabled, auto_video) = (settings.plugins.adblock, settings.auto_video);
    drop(settings);
    if let Some(w) = app.get_webview("player") {
        let _ = w.eval(format!(
            "window.__lazytune?.configure({{adblock:{enabled},auto_video:{auto_video}}})"
        ));
    }
}
pub fn create_player(app: &AppHandle) -> Result<(), String> {
    if app.get_webview("player").is_some() {
        return Ok(());
    }
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let mut builder = tauri::webview::WebviewBuilder::new(
        "player",
        tauri::WebviewUrl::External("https://music.youtube.com/".parse().unwrap()),
    )
    .initialization_script(format!(
        "window.__lazytuneConfig={{adblock:{},auto_video:{},volume:{}}};\n{}",
        settings.plugins.adblock,
        settings.auto_video,
        serde_json::to_string(&settings.volume).map_err(|e| e.to_string())?,
        include_str!("../player.js")
    ))
    .on_navigation(|url| {
        url.scheme() == "https"
            && matches!(
                url.host_str(),
                Some(
                    "music.youtube.com"
                        | "accounts.google.com"
                        | "www.youtube.com"
                        | "consent.youtube.com"
                        | "consent.google.com"
                )
            )
    })
    .on_page_load(|window, _| configure_player(window.app_handle()));
    #[cfg(debug_assertions)]
    if std::env::var_os("LAZYTUNE_INSPECT").is_some() {
        builder = builder.additional_browser_args(
            "--remote-debugging-port=9223 --autoplay-policy=no-user-gesture-required",
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.data_directory(app.state::<AppState>().directory.join("webview"));
    }
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .data_store_identifier(*b"lazytune-player1")
            .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        // WebKit's default user agent lacks Safari identifiers and YouTube rejects it.
        builder=builder.user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15");
    }
    let parent = app.get_window("main").ok_or("Main window unavailable")?;
    let view = parent
        .add_child(
            builder,
            tauri::LogicalPosition::new(-10000.0, -10000.0),
            tauri::LogicalSize::new(960.0, 600.0),
        )
        .map_err(|e| e.to_string())?;
    view.hide().map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn request(app: &AppHandle, kind: &str, value: Value) -> Result<Value, String> {
    let s = app.state::<AppState>();
    let id = s.next_id.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    {
        let mut pending = s.pending.lock().map_err(|_| "Player state unavailable")?;
        if pending.len() >= 24 {
            return Err("Too many pending requests; try again shortly".into());
        }
        pending.insert(id, tx);
    }
    let result = (|| {
        let window = app
            .get_webview("player")
            .ok_or("Player window is unavailable")?;
        let encoded = serde_json::to_string(&(id, kind, value)).map_err(|e| e.to_string())?;
        window.eval(format!("window.__lazytune ? window.__lazytune.request(...{encoded}) : window.__TAURI_INTERNALS__?.invoke('player_report',{{report:JSON.stringify({{kind:'reply',id:{id},error:'YouTube Music is still starting'}})}})" )).map_err(|e|e.to_string())
    })();
    if let Err(e) = result {
        s.pending.lock().unwrap().remove(&id);
        return Err(e);
    }
    let result = tokio::time::timeout(Duration::from_secs(20), rx).await;
    s.pending.lock().unwrap().remove(&id);
    result
        .map_err(|_| {
            "YouTube Music did not respond. Open the player to check your connection or sign-in."
                .to_string()
        })?
        .map_err(|_| "Player request was interrupted".to_string())?
}
pub async fn control(app: &AppHandle, c: PlayerCommand) -> Result<(), String> {
    c.validate()?;
    let volume = if let PlayerCommand::Volume(value) = &c {
        Some(*value)
    } else {
        None
    };
    request(
        app,
        "command",
        serde_json::to_value(c).map_err(|e| e.to_string())?,
    )
    .await?;
    if let Some(volume) = volume {
        let state = app.state::<AppState>();
        state.settings.lock().unwrap().volume = Some(volume);
        let revision = state.volume_revision.fetch_add(1, Ordering::SeqCst) + 1;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            // Coalesce slider updates into one disk write; normal exit also flushes settings.
            tokio::time::sleep(Duration::from_millis(500)).await;
            let state = app.state::<AppState>();
            if state.volume_revision.load(Ordering::SeqCst) == revision
                && !state.preserve_settings.load(Ordering::Relaxed)
            {
                let settings = state.settings.lock().unwrap();
                if let Err(e) =
                    storage::atomic_json(&state.directory.join("settings.json"), &*settings)
                {
                    let _ = app.emit_to("main", "notice", format!("Could not save volume: {e}"));
                }
            }
        });
    }
    Ok(())
}

#[tauri::command]
pub fn bootstrap(window: Webview, state: tauri::State<AppState>) -> Result<Bootstrap, String> {
    local(&window)?;
    Ok(Bootstrap {
        platform: std::env::consts::OS.into(),
        settings: state.settings.lock().unwrap().clone(),
        playback: state.playback.lock().unwrap().clone(),
        warning: state.warning.lock().unwrap().clone(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}
#[tauri::command]
pub async fn catalog(
    window: Webview,
    app: AppHandle,
    query: CatalogRequest,
) -> Result<CatalogPage, String> {
    local(&window)?;
    query.validate()?;
    let value = request(
        &app,
        "catalog",
        serde_json::to_value(query).map_err(|e| e.to_string())?,
    )
    .await?;
    Ok(parse_catalog(&value))
}
#[tauri::command]
pub async fn player_command(
    window: Webview,
    app: AppHandle,
    command: PlayerCommand,
) -> Result<(), String> {
    local(&window)?;
    control(&app, command).await
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Report {
    Snapshot {
        snapshot: PlaybackSnapshot,
    },
    Reply {
        id: u64,
        #[serde(default)]
        result: Value,
        error: Option<String>,
    },
}
#[tauri::command]
pub fn player_report(window: Webview, app: AppHandle, report: String) -> Result<(), String> {
    if window.label() != "player"
        || window
            .url()
            .map_err(|e| e.to_string())?
            .origin()
            .ascii_serialization()
            != "https://music.youtube.com"
    {
        return Err("Untrusted player origin".into());
    }
    if report.len() > 4 * 1024 * 1024 {
        return Err("Player message exceeds limit".into());
    }
    let s = app.state::<AppState>();
    match serde_json::from_str::<Report>(&report).map_err(|_| "Invalid player message")? {
        Report::Snapshot { snapshot } => {
            snapshot.validate()?;
            *s.playback.lock().unwrap() = snapshot.clone();
            #[cfg(target_os = "windows")]
            crate::taskbar::update(&app);
            let _ = app.emit_to("main", "playback", &snapshot);
        }
        Report::Reply { id, result, error } => {
            if let Some(tx) = s.pending.lock().unwrap().remove(&id) {
                let _ = tx.send(match error {
                    Some(e) => Err(e.chars().take(400).collect()),
                    None => Ok(result),
                });
            }
        }
    }
    Ok(())
}
pub fn persist(app: &AppHandle, mut settings: Settings) -> Result<(), String> {
    if settings.version != 1 {
        return Err("Unsupported settings version".into());
    }
    settings.plugins.validate()?;
    if let Some(volume) = settings.volume {
        PlayerCommand::Volume(volume).validate()?;
    }
    settings.theme.validate()?;
    let s = app.state::<AppState>();
    let mut current = s.settings.lock().unwrap();
    settings.volume = current.volume;
    storage::atomic_json(&s.directory.join("settings.json"), &settings)?;
    *current = settings;
    s.preserve_settings.store(false, Ordering::Relaxed);
    drop(current);
    s.generation.fetch_add(1, Ordering::SeqCst);
    s.changed.notify_waiters();
    configure_player(app);
    Ok(())
}
#[tauri::command]
pub fn save_settings(
    window: Webview,
    app: AppHandle,
    plugins: PluginSettings,
    close_to_tray: bool,
    sidebar_collapsed: bool,
    auto_video: bool,
) -> Result<(), String> {
    local(&window)?;
    let mut s = app.state::<AppState>().settings.lock().unwrap().clone();
    s.plugins = plugins;
    s.close_to_tray = close_to_tray;
    s.sidebar_collapsed = sidebar_collapsed;
    s.auto_video = auto_video;
    persist(&app, s)
}
#[tauri::command]
pub fn window_action(window: Webview, action: String) -> Result<(), String> {
    local(&window)?;
    let window = window.window();
    match action.as_str() {
        "minimize" => window.minimize(),
        "maximize" if window.is_maximized().map_err(|e| e.to_string())? => window.unmaximize(),
        "maximize" => window.maximize(),
        "close" => window.close(),
        "drag" => window.start_dragging(),
        _ => return Err("Unknown window action".into()),
    }
    .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn import_theme(window: Webview) -> Result<Option<ThemePack>, String> {
    local(&window)?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .add_filter("lazytune theme", &["lazytune-theme"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path = file.path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = storage::read_limited(&path, MAX_THEME_BYTES)?;
        storage::validate_theme(ThemePack::parse(&bytes)?).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_theme(window: Webview, app: AppHandle) -> Result<(), String> {
    local(&window)?;
    let theme = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .theme
        .clone();
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_file_name("lazytune.lazytune-theme")
        .add_filter("lazytune theme", &["lazytune-theme"])
        .save_file()
        .await
    else {
        return Ok(());
    };
    let path = file.path().to_owned();
    tauri::async_runtime::spawn_blocking(move || storage::atomic_json(&path, &theme))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn apply_theme(
    window: Webview,
    app: AppHandle,
    theme: ThemePack,
) -> Result<ThemePack, String> {
    local(&window)?;
    let theme = tauri::async_runtime::spawn_blocking(move || storage::validate_theme(theme))
        .await
        .map_err(|e| e.to_string())??;
    let mut s = app.state::<AppState>().settings.lock().unwrap().clone();
    s.theme = theme.clone();
    persist(&app, s)?;
    Ok(theme)
}
#[tauri::command]
pub async fn choose_wallpaper(
    window: Webview,
    app: AppHandle,
) -> Result<Option<ThemePack>, String> {
    local(&window)?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .add_filter("Static image", &["png", "jpg", "jpeg"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path = file.path().to_owned();
    let mut theme = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .theme
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        theme.wallpaper = Some(storage::canonical_wallpaper(&storage::read_limited(
            &path,
            MAX_IMAGE_BYTES,
        )?)?);
        Ok(Some(theme))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn reset_theme(window: Webview, app: AppHandle) -> Result<ThemePack, String> {
    local(&window)?;
    let mut s = app.state::<AppState>().settings.lock().unwrap().clone();
    s.theme = ThemePack::default();
    let theme = s.theme.clone();
    persist(&app, s)?;
    Ok(theme)
}
#[tauri::command]
pub async fn show_player(
    window: Webview,
    app: AppHandle,
    mode: String,
    bounds: Option<SurfaceBounds>,
) -> Result<(), String> {
    local(&window)?;
    if !["login", "video", "audio"].contains(&mode.as_str()) {
        return Err("Invalid player mode".into());
    }
    let view = app
        .get_webview("player")
        .ok_or("Player is still starting")?;
    let previous = std::mem::replace(
        &mut *app.state::<AppState>().player_mode.lock().unwrap(),
        mode.clone(),
    );
    if mode == "audio" {
        view.eval("window.__lazytune?.mode('audio')")
            .map_err(|e| e.to_string())?;
        return view.hide().map_err(|e| e.to_string());
    }
    if mode == "login"
        && previous != "login"
        && !app.state::<AppState>().playback.lock().unwrap().signed_in
        && view.url().map_err(|e| e.to_string())?.host_str() == Some("music.youtube.com")
    {
        let mut next = url::Url::parse("https://www.youtube.com/signin").unwrap();
        next.query_pairs_mut()
            .append_pair("action_handle_signin", "true")
            .append_pair("app", "desktop")
            .append_pair("next", "https://music.youtube.com/");
        let mut login = url::Url::parse("https://accounts.google.com/ServiceLogin").unwrap();
        login
            .query_pairs_mut()
            .append_pair("service", "youtube")
            .append_pair("continue", next.as_str());
        view.navigate(login).map_err(|e| e.to_string())?;
    }
    let bounds = bounds.ok_or("Missing player position")?;
    bounds.validate()?;
    let native = window.window();
    let size = native
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(native.scale_factor().map_err(|e| e.to_string())?);
    if bounds.x + bounds.width > size.width + 1.0 || bounds.y + bounds.height > size.height + 1.0 {
        return Err("Player surface is outside the window".into());
    }
    view.set_bounds(tauri::Rect {
        position: tauri::LogicalPosition::new(bounds.x, bounds.y).into(),
        size: tauri::LogicalSize::new(bounds.width, bounds.height).into(),
    })
    .map_err(|e| e.to_string())?;
    view.eval(format!(
        "window.__lazytune?.mode({})",
        serde_json::to_string(&mode).unwrap()
    ))
    .map_err(|e| e.to_string())?;
    view.show().map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn logout(window: Webview, app: AppHandle) -> Result<(), String> {
    local(&window)?;
    if let Some(w) = app.get_webview("player") {
        w.clear_all_browsing_data().map_err(|e| e.to_string())?;
        w.navigate("https://music.youtube.com/".parse().unwrap())
            .map_err(|e| e.to_string())?;
    }
    *app.state::<AppState>().playback.lock().unwrap() = PlaybackSnapshot::default();
    Ok(())
}
#[tauri::command]
pub fn plugin_status(
    window: Webview,
    state: tauri::State<AppState>,
) -> Result<Vec<PluginStatus>, String> {
    local(&window)?;
    Ok(state.statuses.lock().unwrap().values().cloned().collect())
}
#[tauri::command]
pub async fn lyrics(window: Webview, app: AppHandle) -> Result<Lyrics, String> {
    local(&window)?;
    plugins::lyrics(&app).await
}
#[tauri::command]
pub async fn artwork_colors(window: Webview, app: AppHandle) -> Result<Vec<String>, String> {
    local(&window)?;
    plugins::artwork_colors(&app).await
}
#[tauri::command]
pub async fn set_credential(
    window: Webview,
    app: AppHandle,
    service: String,
    value: String,
) -> Result<(), String> {
    local(&window)?;
    if service != "listenbrainz" || value.len() > 256 {
        return Err("Invalid credential".into());
    }
    let enabled = !value.is_empty();
    tauri::async_runtime::spawn_blocking(move || {
        let entry = storage::secret("listenbrainz")?;
        if enabled {
            entry
                .set_password(value.trim())
                .map_err(|_| "Could not save token".to_string())
        } else {
            entry
                .delete_credential()
                .map_err(|_| "Could not remove token".to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    let s = app.state::<AppState>();
    s.generation.fetch_add(1, Ordering::SeqCst);
    s.changed.notify_waiters();
    Ok(())
}
#[tauri::command]
pub async fn lastfm_connect(window: Webview, app: AppHandle) -> Result<String, String> {
    local(&window)?;
    plugins::lastfm_connect(&app).await
}
#[tauri::command]
pub async fn lastfm_finish(window: Webview, app: AppHandle, token: String) -> Result<(), String> {
    local(&window)?;
    plugins::lastfm_finish(&app, &token).await
}
#[tauri::command]
pub async fn undo_skip(window: Webview, app: AppHandle) -> Result<(), String> {
    local(&window)?;
    let undo = app.state::<AppState>().undo.lock().unwrap().take();
    if let Some((id, start, end)) = undo {
        let current = app
            .state::<AppState>()
            .playback
            .lock()
            .unwrap()
            .track
            .id
            .clone();
        if current == id {
            let _ = app.emit_to("main", "notice", "SponsorBlock skip undone");
            app.state::<AppState>()
                .status("sponsorblock", "undo", &format!("{end}"));
            control(
                &app,
                PlayerCommand::Seek {
                    position: start,
                    track_id: id,
                },
            )
            .await?;
        }
    }
    Ok(())
}
