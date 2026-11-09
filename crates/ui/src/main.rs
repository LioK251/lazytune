use lazytune_model::*;
use leptos::{html, prelude::*};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{spawn_local, JsFuture};
mod settings;
mod slider;
use settings::SettingsView;
use slider::PlaybackSlider;

fn text_selected(event: &web_sys::MouseEvent) -> bool {
    let button = event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .and_then(|element| element.closest("button").ok().flatten());
    let selection = web_sys::window().and_then(|w| w.get_selection().ok().flatten());
    match (button, selection) {
        (Some(button), Some(selection)) => {
            event.detail() > 0
                && !selection.is_collapsed()
                && button.contains(selection.anchor_node().as_ref())
        }
        _ => false,
    }
}

async fn call<T: DeserializeOwned>(command: &str, args: Value) -> Result<T, String> {
    let window = web_sys::window().ok_or("Window unavailable")?;
    let tauri = js_sys::Reflect::get(&window, &"__TAURI__".into())
        .map_err(|_| "Desktop connection unavailable")?;
    let core = js_sys::Reflect::get(&tauri, &"core".into())
        .map_err(|_| "Open the desktop app to connect to YouTube Music")?;
    let invoke = js_sys::Reflect::get(&core, &"invoke".into())
        .map_err(|_| "Desktop connection unavailable")?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "Desktop connection unavailable")?;
    let args = args
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| e.to_string())?;
    let promise = invoke
        .call2(&core, &command.into(), &args)
        .map_err(|e| {
            e.as_string()
                .unwrap_or("Desktop connection unavailable".into())
        })?
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| "Invalid desktop response")?;
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| e.as_string().unwrap_or("Request failed".into()))?;
    serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())
}
async fn listen<T: DeserializeOwned + 'static>(
    event: &str,
    mut callback: impl FnMut(T) + 'static,
) -> Result<(), String> {
    let window = web_sys::window().ok_or("Window unavailable")?;
    let tauri =
        js_sys::Reflect::get(&window, &"__TAURI__".into()).map_err(|_| "Desktop unavailable")?;
    let events = js_sys::Reflect::get(&tauri, &"event".into()).map_err(|_| "Events unavailable")?;
    let function = js_sys::Reflect::get(&events, &"listen".into())
        .map_err(|_| "Events unavailable")?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "Events unavailable")?;
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        if let Ok(payload) = js_sys::Reflect::get(&event, &"payload".into()) {
            if let Ok(value) = serde_wasm_bindgen::from_value(payload) {
                callback(value);
            }
        }
    });
    let promise = function
        .call2(&events, &event.into(), closure.as_ref())
        .map_err(|_| "Could not subscribe")?
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| "Could not subscribe")?;
    JsFuture::from(promise)
        .await
        .map_err(|_| "Could not subscribe")?;
    closure.forget();
    Ok(())
}
#[derive(Clone)]
struct HistoryEntry {
    route: String,
    title: String,
    query: CatalogRequest,
    page: CatalogPage,
    artwork: String,
}
#[derive(Clone, Copy)]
struct Ui {
    settings: RwSignal<Settings>,
    playback: RwSignal<PlaybackSnapshot>,
    track: Memo<lazytune_model::Track>,
    queue: Memo<Vec<lazytune_model::Track>>,
    platform: RwSignal<String>,
    saving: RwSignal<bool>,
    surface_bounds: RwSignal<Option<(String, SurfaceBounds)>>,
    colors: RwSignal<Vec<String>>,
    appearance_error: RwSignal<String>,
    draft_revision: RwSignal<u64>,
    route: RwSignal<String>,
    page: RwSignal<CatalogPage>,
    home_library: RwSignal<CatalogPage>,
    loading: RwSignal<bool>,
    error: RwSignal<String>,
    notice: RwSignal<String>,
    query: RwSignal<String>,
    generation: RwSignal<u64>,
    lyrics: RwSignal<Lyrics>,
    lyric_id: RwSignal<String>,
    statuses: RwSignal<Vec<PluginStatus>>,
    draft: RwSignal<ThemePack>,
    preview: RwSignal<Option<ThemePack>>,
    settings_tab: RwSignal<String>,
    lastfm_token: RwSignal<String>,
    browse_title: RwSignal<String>,
    browse_artwork: RwSignal<String>,
    current_query: RwSignal<CatalogRequest>,
    history: RwSignal<Vec<HistoryEntry>>,
}
impl Ui {
    fn window_action(self, action: &'static str) {
        spawn_local(async move {
            self.message(call::<()>("window_action", json!({"action":action})).await);
        });
    }
    fn message(self, result: Result<(), String>) {
        if let Err(e) = result {
            self.notice.set(e)
        }
    }
    fn command(self, command: PlayerCommand) {
        spawn_local(async move {
            self.message(call::<()>("player_command", json!({"command":command})).await);
        });
    }
    fn show_player(self, mode: &str) {
        self.navigate(if mode == "video" {
            "video"
        } else if mode == "login" {
            "account"
        } else {
            "player"
        });
    }
    fn position_player(self) {
        let mode = match self.route.get_untracked().as_str() {
            "video" => "video",
            "signin" => "login",
            _ => return,
        };
        let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("player-surface"))
        else {
            return;
        };
        let r = element.get_bounding_client_rect();
        let bounds = SurfaceBounds {
            x: r.x(),
            y: r.y(),
            width: r.width(),
            height: r.height(),
        };
        if bounds.validate().is_err() {
            return;
        }
        let next = (mode.to_string(), bounds.clone());
        if self
            .surface_bounds
            .with_untracked(|old| old.as_ref() == Some(&next))
        {
            return;
        }
        self.surface_bounds.set(Some(next));
        spawn_local(async move {
            if let Err(e) = call::<()>("show_player", json!({"mode":mode,"bounds":bounds})).await {
                self.surface_bounds.set(None);
                self.notice.set(e);
            }
        });
    }
    fn navigate(self, route: &str) {
        self.generation.update(|g| *g += 1);
        self.loading.set(false);
        self.route.set(route.into());
        self.error.set(String::new());
        match route {
            "home" => self.load(CatalogRequest::Home, false),
            "library" => self.load(CatalogRequest::Library, false),
            "liked" => self.load(CatalogRequest::Liked, false),
            "settings" => self.draft.set(self.settings.get_untracked().theme),
            "player" => self.load_lyrics(),
            _ => {}
        }
    }
    fn load(self, request: CatalogRequest, append: bool) {
        self.current_query.set(request.clone());
        self.loading.set(true);
        self.error.set(String::new());
        self.generation.update(|g| *g += 1);
        let generation = self.generation.get_untracked();
        spawn_local(async move {
            let result = call::<CatalogPage>("catalog", json!({"query":request})).await;
            if self.generation.get_untracked() != generation {
                return;
            }
            self.loading.set(false);
            match result {
                Ok(page) => {
                    if append {
                        self.page.update(|p| {
                            p.shelves.extend(page.shelves);
                            p.continuation = page.continuation;
                        });
                    } else {
                        if self.route.get_untracked() == "browse" && !page.artwork.is_empty() {
                            self.browse_artwork.set(page.artwork.clone());
                        }
                        self.page.set(page);
                    }
                }
                Err(e) => self.error.set(e),
            }
            if self.route.get_untracked() == "home" {
                if let Ok(page) =
                    call::<CatalogPage>("catalog", json!({"query":CatalogRequest::Library})).await
                {
                    self.home_library.set(page);
                }
            }
        });
    }
    fn open(self, item: CatalogItem) {
        if item.playable {
            self.command(PlayerCommand::Play(item.id));
            self.navigate("player");
        } else {
            self.history.update(|h| {
                h.push(HistoryEntry {
                    route: self.route.get_untracked(),
                    title: self.browse_title.get_untracked(),
                    query: self.current_query.get_untracked(),
                    page: self.page.get_untracked(),
                    artwork: self.browse_artwork.get_untracked(),
                })
            });
            self.browse_title.set(item.title);
            self.browse_artwork.set(item.artwork);
            self.route.set("browse".into());
            self.load(CatalogRequest::Browse(item.id), false);
        }
    }
    fn back(self) {
        if let Some(HistoryEntry {
            route,
            title,
            query,
            page,
            artwork,
        }) = self.history.try_update(|h| h.pop()).flatten()
        {
            self.generation.update(|g| *g += 1);
            self.route.set(route);
            self.browse_title.set(title);
            self.browse_artwork.set(artwork);
            self.current_query.set(query);
            self.page.set(page);
            self.loading.set(false);
            self.error.set(String::new());
        } else {
            self.navigate("home");
        }
    }
    fn load_lyrics(self) {
        let p = self.playback.get_untracked();
        if p.track.id.is_empty()
            || p.duration <= 0.0
            || !self.settings.with_untracked(|s| s.plugins.lyrics)
            || self.lyric_id.get_untracked() == p.track.id
        {
            return;
        }
        let id = p.track.id;
        self.lyric_id.set(id.clone());
        self.lyrics.set(Lyrics::default());
        spawn_local(async move {
            match call::<Lyrics>("lyrics", json!({})).await {
                Ok(l) if self.playback.get_untracked().track.id == id => self.lyrics.set(l),
                Err(e) if self.playback.with_untracked(|p| p.track.id == id) => {
                    self.lyrics.set(Lyrics {
                        source: e,
                        ..Lyrics::default()
                    })
                }
                _ => {}
            }
        });
    }
    fn save_plugins(self) {
        if self.saving.get_untracked() {
            return;
        }
        self.saving.set(true);
        let (plugins, close_to_tray, sidebar_collapsed, auto_video) =
            self.settings.with_untracked(|s| {
                (
                    s.plugins.clone(),
                    s.close_to_tray,
                    s.sidebar_collapsed,
                    s.auto_video,
                )
            });
        spawn_local(async move {
            if let Err(e) = call::<()>(
                    "save_settings",
                    json!({"plugins":plugins,"closeToTray":close_to_tray,"sidebarCollapsed":sidebar_collapsed,"autoVideo":auto_video}),
                )
                .await {
                self.notice.set(e);
                if let Ok(b) = call::<Bootstrap>("bootstrap",json!({})).await {self.settings.set(b.settings);}
                self.saving.set(false);
                return;
            }
            self.saving.set(false);
            if self.settings.with_untracked(|s| {
                s.plugins != plugins
                    || s.close_to_tray != close_to_tray
                    || s.sidebar_collapsed != sidebar_collapsed
                    || s.auto_video != auto_video
            }) {
                self.save_plugins();
            }
        });
    }
    fn apply(self, theme: ThemePack) {
        spawn_local(async move {
            let requested = theme.clone();
            match call::<ThemePack>("apply_theme", json!({"theme":theme})).await {
                Ok(t) => {
                    self.settings.update(|s| s.theme = t.clone());
                    if self.draft.with_untracked(|d| *d == requested)
                        || self.preview.with_untracked(Option::is_some)
                    {
                        self.draft.set(t);
                    }
                    self.preview.set(None);
                    self.appearance_error.set(String::new());
                }
                Err(e) => self.appearance_error.set(e),
            }
        });
    }
}
fn time(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
fn theme_style(t: &ThemePack) -> String {
    format!(
        "--bg:{};--surface:{};--text:{};--muted:{};--accent:{};--dim:{};--blur:{}px;--space:{}px",
        t.palette.background,
        t.palette.surface,
        t.palette.text,
        t.palette.muted,
        t.palette.accent,
        t.dim,
        t.blur,
        if t.density == Density::Compact {
            16
        } else {
            24
        }
    )
}
#[component]
fn Icon(#[prop(into)] name: String) -> impl IntoView {
    let path=match name.as_str(){"play"=>"m9 5 11 7-11 7z","pause"=>"M8 5v14M16 5v14","next"=>"m5 5 10 7-10 7zM19 5v14","previous"=>"m19 5-10 7 10 7zM5 5v14","search"=>"M21 21l-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0","home"=>"m3 10 9-7 9 7v11h-6v-7H9v7H3z","library"=>"M4 4v16M9 4v16M14 4v16m4-15 3 14","heart"=>"M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8Z","settings"=>"M4 7h16M4 17h16M8 4v6M16 14v6","volume"=>"m3 9 5 0 5-4v14l-5-4H3zM17 8a6 6 0 0 1 0 8M20 5a10 10 0 0 1 0 14","video"=>"M3 5h14v14H3zM17 9l5-3v12l-5-3","plus"=>"M12 5v14M5 12h14","back"=>"m14 6-6 6 6 6","close"=>"m6 6 12 12M6 18 18 6","shuffle"=>"M3 5h3l12 14h3m-4-4 4 4-4 3M3 19h3L18 5h3m-4-3 4 3-4 4","repeat"=>"m17 2 4 4-4 4M3 11V9a3 3 0 0 1 3-3h15M7 22l-4-4 4-4m14-1v2a3 3 0 0 1-3 3H3","user"=>"M20 21v-2a7 7 0 0 0-14 0v2M17 7a5 5 0 1 1-10 0 5 5 0 0 1 10 0","music"=>"M9 18V5l12-2v13M9 8l12-2M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0m12-2a3 3 0 1 1-6 0 3 3 0 0 1 6 0",_=>"M5 12h14"};
    view! {<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d=path/></svg>}
}
#[component]
fn Cover(#[prop(into)] url: String, #[prop(optional)] large: bool) -> impl IntoView {
    let failed = RwSignal::new(false);
    let fallback = move || view! {<div class=if large{"cover fallback large"}else{"cover fallback"}><Icon name="music"/></div>};
    if url.is_empty() {
        fallback().into_any()
    } else {
        view!{<Show when=move||!failed.get() fallback><img class=if large{"cover large"}else{"cover"} src=url.clone() alt="Album artwork" draggable="false" loading="lazy" on:error=move|_|failed.set(true)/></Show>}.into_any()
    }
}
#[component]
fn Nav(
    ui: Ui,
    #[prop(into)] route: String,
    #[prop(into)] label: String,
    #[prop(into)] icon: String,
) -> impl IntoView {
    let check = route.clone();
    let current = route.clone();
    view! {<button aria-label=label.clone() title=label.clone() class:active=move||ui.route.get()==check||(check=="player"&&ui.route.get()=="video") aria-current=move||if ui.route.get()==current||(current=="player"&&ui.route.get()=="video"){Some("page")}else{None} on:click=move|_|ui.navigate(&route)><Icon name=icon/><span>{label.clone()}</span></button>}
}
#[component]
fn App() -> impl IntoView {
    let playback = RwSignal::new(PlaybackSnapshot::default());
    let ui = Ui {
        settings: RwSignal::new(Settings::default()),
        playback,
        track: Memo::new(move |_| playback.with(|p| p.track.clone())),
        queue: Memo::new(move |_| playback.with(|p| p.queue.clone())),
        platform: RwSignal::new(String::new()),
        saving: RwSignal::new(false),
        surface_bounds: RwSignal::new(None),
        colors: RwSignal::new(vec![]),
        appearance_error: RwSignal::new(String::new()),
        draft_revision: RwSignal::new(0),
        route: RwSignal::new("home".into()),
        page: RwSignal::new(CatalogPage::default()),
        home_library: RwSignal::new(CatalogPage::default()),
        loading: RwSignal::new(false),
        error: RwSignal::new(String::new()),
        notice: RwSignal::new(String::new()),
        query: RwSignal::new(String::new()),
        generation: RwSignal::new(0),
        lyrics: RwSignal::new(Lyrics::default()),
        lyric_id: RwSignal::new(String::new()),
        statuses: RwSignal::new(vec![]),
        draft: RwSignal::new(ThemePack::default()),
        preview: RwSignal::new(None),
        settings_tab: RwSignal::new("appearance".into()),
        lastfm_token: RwSignal::new(String::new()),
        browse_title: RwSignal::new(String::new()),
        browse_artwork: RwSignal::new(String::new()),
        current_query: RwSignal::new(CatalogRequest::Home),
        history: RwSignal::new(vec![]),
    };
    let dialog = NodeRef::<html::Dialog>::new();
    Effect::new(move || {
        let notice = ui.notice.get();
        if !notice.is_empty() {
            set_timeout(
                move || {
                    if ui.notice.get_untracked() == notice {
                        ui.notice.set(String::new());
                    }
                },
                Duration::from_secs(5),
            );
        }
    });
    Effect::new(move || {
        let draft = ui.draft.get();
        ui.draft_revision.update(|n| *n += 1);
        let revision = ui.draft_revision.get_untracked();
        if ui.settings.with_untracked(|s| s.theme == draft) {
            return;
        }
        set_timeout(
            move || {
                if ui.draft_revision.get_untracked() != revision {
                    return;
                }
                match draft.validate() {
                    Ok(()) => ui.apply(draft),
                    Err(e) => ui.appearance_error.set(e),
                }
            },
            Duration::from_millis(400),
        );
    });
    let wallpaper = Memo::new(move |_| {
        ui.settings.with(|s| {
            s.theme
                .wallpaper
                .as_ref()
                .map(|w| format!("data:image/{};base64,{}", w.format, w.data))
        })
    });
    let artwork = Memo::new(move |_| ui.track.with(|t| t.artwork.clone()));
    Effect::new(move || {
        let image = artwork.get();
        if image.is_empty() || !ui.settings.with(|s| s.theme.song_colors) {
            ui.colors.set(vec![]);
            return;
        }
        spawn_local(async move {
            if let Ok(colors) = call::<Vec<String>>("artwork_colors", json!({})).await {
                if ui.track.with_untracked(|t| t.artwork == image) {
                    ui.colors.set(colors);
                }
            }
        });
    });
    Effect::new(move || {
        let route = ui.route.get();
        ui.surface_bounds.set(None);
        if route != "video" && route != "signin" {
            spawn_local(async move {
                let _ = call::<()>("show_player", json!({"mode":"audio","bounds":null})).await;
            });
        }
    });
    set_interval(move || ui.position_player(), Duration::from_millis(120));
    Effect::new(move || {
        let open = ui.preview.get().is_some();
        if let Some(d) = dialog.get() {
            if open {
                let _ = d.show_modal();
            } else {
                d.close();
            }
        }
    });
    spawn_local(async move {
        match call::<Bootstrap>("bootstrap", json!({})).await {
            Ok(b) => {
                ui.platform.set(b.platform);
                ui.draft.set(b.settings.theme.clone());
                ui.settings.set(b.settings);
                ui.playback.set(b.playback);
                if let Some(w) = b.warning {
                    ui.notice.set(w);
                }
            }
            Err(e) => {
                ui.error.set(e);
                return;
            }
        }
        let mut first = true;
        if let Err(e) = listen::<PlaybackSnapshot>("playback", move |p| {
            let metadata_ready =
                ui.playback.with_untracked(|old| old.duration <= 0.0) && p.duration > 0.0;
            let id_changed = ui.playback.with_untracked(|old| old.track.id != p.track.id);
            let ready = p.ready;
            let signed_in = p.signed_in;
            ui.playback.set(p);
            if signed_in && ui.route.get_untracked() == "signin" {
                ui.navigate("account");
            }
            if ready && first {
                first = false;
                if ui.route.get_untracked() == "home" {
                    ui.load(CatalogRequest::Home, false);
                }
            }
            if (id_changed || metadata_ready) && ui.route.get_untracked() == "player" {
                ui.load_lyrics();
            }
        })
        .await
        {
            ui.error.set(e)
        }
        let _ = listen::<String>("notice", move |n| ui.notice.set(n)).await;
        set_interval(
            move || {
                spawn_local(async move {
                    if let Ok(s) = call::<Vec<PluginStatus>>("plugin_status", json!({})).await {
                        ui.statuses.set(s);
                    }
                })
            },
            Duration::from_secs(5),
        );
    });
    view! {<div class="app" class:glassy-player=move||ui.settings.with(|s|s.theme.glassy_player) class:song-colors=move||ui.settings.with(|s|s.theme.song_colors)&&ui.colors.with(|c|c.len()==4) style=move||{
        let mut style=ui.preview.with(|p|p.as_ref().map(theme_style).unwrap_or_else(||ui.settings.with(|s|theme_style(&s.theme))));
        ui.colors.with(|c|{if c.len()==4&&ui.settings.with(|s|s.theme.song_colors){style.push_str(&format!(";--song-a:{};--song-b:{};--song-c:{};--accent:{};--bg:#171717;--surface:#222222;--text:#DDDDDD;--muted:#BBBBBB",c[0],c[1],c[2],c[3]));}});
        style
    }>
      <div class="artwork-background">{move||{let source=match ui.route.get().as_str(){"browse"=>ui.browse_artwork.get(),"player"=>wallpaper.get().unwrap_or_else(||if ui.settings.with(|s|s.theme.artwork_colors){ui.track.with(|t|t.artwork.clone())}else{String::new()}),_=>String::new()};if source.is_empty(){None}else{Some(view!{<img src=source alt=""/>})}}}</div>
      <a class="skip-link" href="#content">"Skip to content"</a><Show when=move||{let p=ui.platform.get();!p.is_empty()&&p!="macos"}><header class="titlebar"><div class="title-drag" on:mousedown=move|e:web_sys::MouseEvent|{if e.button()==0{if e.detail()==2{ui.window_action("maximize")}else{ui.window_action("drag")}}}/><div class="window-controls"><button aria-label="Minimize window" on:click=move|_|ui.window_action("minimize")><svg viewBox="0 0 16 16"><path d="M3 8h10"/></svg></button><button aria-label="Maximize or restore window" on:click=move|_|ui.window_action("maximize")><svg viewBox="0 0 16 16"><rect x="3.5" y="3.5" width="9" height="9"/></svg></button><button class="window-close" aria-label="Close window" on:click=move|_|ui.window_action("close")><Icon name="close"/></button></div></header></Show><div class="shell" class:sidebar-collapsed=move||ui.settings.with(|s|s.sidebar_collapsed)><aside class="sidebar">
        <a class="brand" href="#" on:click=move|e|{e.prevent_default();ui.navigate("home");}><svg class="brand-mark" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 4h3.5v12.5h7.3V20H5z" fill="currentColor"/><path d="m12 5 8 4.7-8 4.7z" fill="currentColor" opacity="0.65"/></svg><span>"lazytune"</span></a>
        <nav aria-label="Main navigation"><Nav ui route="home" label="Discover" icon="home"/><Nav ui route="library" label="Library" icon="library"/><Nav ui route="liked" label="Liked Music" icon="heart"/><Nav ui route="player" label="Now Playing" icon="music"/></nav>
        <div class="sidebar-bottom"><button class="account" class:active=move||matches!(ui.route.get().as_str(),"account"|"signin") aria-label="Account" title="Account" on:click=move|_|ui.show_player("login")><Icon name="user"/><span>"Account"</span></button><Nav ui route="settings" label="Settings" icon="settings"/></div>
      </aside><div class="workspace"><header class="topbar"><button class="icon-button sidebar-toggle" aria-label=move||if ui.settings.with(|s|s.sidebar_collapsed){"Expand sidebar"}else{"Collapse sidebar"} title="Toggle sidebar" on:click=move|_|{ui.settings.update(|s|s.sidebar_collapsed = !s.sidebar_collapsed);ui.save_plugins();}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/></svg></button><button class="icon-button" title="Go back" aria-label="Go back" on:click=move|_|ui.back()><Icon name="back"/></button>
        <form class="search" on:submit=move|e|{e.prevent_default();let q=ui.query.get_untracked();if !q.trim().is_empty(){ui.route.set("search".into());ui.load(CatalogRequest::Search(q),false);}}><Icon name="search"/><input aria-label="Search music" placeholder="Search songs, artists, albums" prop:value=move||ui.query.get() on:input=move|e|ui.query.set(event_target_value(&e))/></form></header>
        <main id="content" tabindex="-1" class:embedded=move||matches!(ui.route.get().as_str(),"video"|"signin")>{move||match ui.route.get().as_str(){"settings"=>view!{<SettingsView ui/>}.into_any(),"player"=>view!{<PlayerView ui/>}.into_any(),"video"|"signin"=>view!{<EmbeddedPlayer ui/>}.into_any(),"account"=>view!{<AccountView ui/>}.into_any(),_=>view!{<CatalogView ui/>}.into_any()}}</main>
      </div></div><Transport ui/>
      <Show when=move||!ui.notice.get().is_empty()><div class="toast" role="status"><span>{move||ui.notice.get()}</span><Show when=move||ui.notice.with(|n|n.contains("Skipped a SponsorBlock"))><button class="text-button" on:click=move|_|spawn_local(async move{ui.message(call::<()>("undo_skip",json!({})).await);})>"Undo"</button></Show><button class="icon-button" aria-label="Dismiss notification" on:click=move|_|ui.notice.set(String::new())><Icon name="close"/></button></div></Show>
      <dialog class="theme-modal" node_ref=dialog on:cancel=move|_:web_sys::Event|ui.preview.set(None)><h2>{move||ui.preview.get().map(|t|t.name).unwrap_or_default()}</h2><p>"Apply this theme’s colors and layout?"</p><div class="preview-swatches">{move||ui.preview.get().map(|t|vec![t.palette.background,t.palette.surface,t.palette.accent,t.palette.text].into_iter().map(|c|view!{<span style=format!("background:{c}")/>}).collect_view())}</div><div class="actions"><button class="secondary" on:click=move|_|ui.preview.set(None)>"Cancel"</button><button class="primary" on:click=move|_|{if let Some(t)=ui.preview.get_untracked(){ui.apply(t);}}>"Apply theme"</button></div></dialog>
    </div>}
}
#[component]
fn CatalogView(ui: Ui) -> impl IntoView {
    view! {
      <div class="page-heading"><h1 class:selectable=move||ui.route.get()=="browse">{move||match ui.route.get().as_str(){"home"=>"Discover".into(),"search"=>format!("Results for “{}”",ui.query.get()),"library"=>"Library".into(),"liked"=>"Liked Music".into(),_=>ui.browse_title.get()}}</h1><Show when=move||ui.route.get()=="browse"&&!ui.loading.get()&&ui.page.with(|p|p.subscription.is_some())><FollowButton ui/></Show></div>
      <Show when=move||ui.loading.get()><p class="muted" role="status">"Loading…"</p></Show>
      <Show when=move||!ui.error.get().is_empty()><section class="empty-state" role="alert"><p>{move||ui.error.get()}</p><div class="actions"><button class="secondary" on:click=move|_|ui.load(ui.current_query.get_untracked(),false)>"Retry"</button><button class="secondary" on:click=move|_|ui.show_player("login")>"Open YouTube Music"</button></div></section></Show>
      {move||{
        let home = ui.route.get()=="home";
        let sections = if home {ui.settings.with(|s|s.theme.layout.home.clone())} else {vec!["recommendations".into()]};
        sections.into_iter().map(|section| {
            let shelves = if section=="library" {ui.home_library.get().shelves} else {ui.page.get().shelves};
            shelves.into_iter().map(|s|view!{<ShelfView ui shelf=s fallback_title=String::new()/>}).collect_view()
        }).collect_view()
      }}
      <Show when=move||ui.page.with(|p|p.continuation.is_some())><button class="secondary load-more" disabled=move||ui.loading.get() on:click=move|_|{if let Some(token)=ui.page.get_untracked().continuation{ui.load(CatalogRequest::Continue{token,search:ui.route.get_untracked()=="search"},true);}}>"Load more"</button></Show>
      <Show when=move||!ui.loading.get()&&ui.error.get().is_empty()&&ui.page.with(|p|p.shelves.is_empty())><section class="empty-state"><p>{move||if ui.route.get()=="search"{"No results."}else{"Search for music, or sign in to see your library."}}</p></section></Show>
    }
}
#[component]
fn ShelfView(ui: Ui, shelf: Shelf, fallback_title: String) -> impl IntoView {
    let limit = RwSignal::new(24usize);
    let title = if shelf.title.is_empty() {
        fallback_title
    } else {
        shelf.title
    };
    let items = StoredValue::new(shelf.items);
    view! {<section class="music-section"><div class="section-heading"><h2>{title}</h2></div><div class="music-grid">{move||items.get_value().into_iter().take(limit.get()).map(|item|{let play=item.clone();let collection=!item.playable&&(item.id.starts_with("VL")||item.id.starts_with("MPRE"));let play_label=if item.playable{"Play track"}else if item.id.starts_with("MPRE"){"Play album"}else{"Play playlist"};let enqueue=StoredValue::new(item.id.clone());view!{<article class="music-card"><button class="card-open" on:click=move|e|{if !text_selected(&e){ui.open(play.clone())}}><Cover url=item.artwork/><strong>{item.title}</strong><small>{item.subtitle}</small></button><Show when=move||item.playable||collection><button class="card-play" aria-label=play_label title="Play" on:click=move|_|ui.command(if item.playable{PlayerCommand::Play(enqueue.get_value())}else{PlayerCommand::PlayCollection(enqueue.get_value())})><Icon name="play"/></button></Show><Show when=move||item.playable><button class="add-queue" aria-label="Add to queue" title="Add to queue" on:click=move|_|ui.command(PlayerCommand::Enqueue(enqueue.get_value()))><Icon name="plus"/></button></Show></article>}}).collect_view()}</div><Show when=move||{items.with_value(Vec::len)>limit.get()}><button class="text-button" on:click=move|_|limit.update(|n|*n+=24)>"Show more"</button></Show></section>}
}
#[component]
fn FollowButton(ui: Ui) -> impl IntoView {
    let saving = RwSignal::new(false);
    view! {<button class="secondary" disabled=move||saving.get() aria-pressed=move||ui.page.with(|p|p.subscription.as_ref().is_some_and(|s|s.followed)).to_string() on:click=move|_|{
        if !ui.playback.with_untracked(|p|p.signed_in) {ui.navigate("account");return;}
        let Some(subscription)=ui.page.with_untracked(|p|p.subscription.clone()) else{return;};
        saving.set(true);
        spawn_local(async move {
            let result=call::<()>("player_command",json!({"command":PlayerCommand::Follow{channel_id:subscription.channel_id.clone(),follow:!subscription.followed}})).await;
            match result {
                Ok(())=>ui.page.update(|p|{if let Some(current)=&mut p.subscription {if current.channel_id==subscription.channel_id{current.followed = !subscription.followed;}}}),
                Err(e)=>ui.notice.set(e),
            }
            saving.try_set(false);
        });
    }>{move||if ui.page.with(|p|p.subscription.as_ref().is_some_and(|s|s.followed)){"Following"}else{"Follow"}}</button>}
}
#[component]
fn ArtistLink(ui: Ui) -> impl IntoView {
    view! {<button class="artist-link" disabled=move||ui.track.with(|t|t.artist_id.is_empty()) on:click=move|e|{
        if !text_selected(&e) {let track=ui.track.get_untracked();ui.open(CatalogItem{id:track.artist_id,title:track.artist, ..CatalogItem::default()});}
    }>{move||ui.track.with(|t|t.artist.clone())}</button>}
}
#[component]
fn Transport(ui: Ui) -> impl IntoView {
    view! {<footer class="transport" aria-label="Playback controls"><div class="current-track"><button class="current-track-cover" aria-label="Open Now Playing" on:click=move|_|ui.navigate("player")>{move||view!{<Cover url=ui.track.get().artwork/>}}</button><span><button class="track-title" on:click=move|e|{if !text_selected(&e){ui.navigate("player")}}><strong>{move||{let t=ui.track.get().title;if t.is_empty(){"Nothing playing".into()}else{t}}}</strong></button><ArtistLink ui/></span></div>
    <div class="transport-center"><div class="transport-buttons">
      <button class="icon-button subtle" aria-label="Shuffle queue" on:click=move|_|ui.command(PlayerCommand::Shuffle)><Icon name="shuffle"/></button>
      <button class="icon-button" aria-label="Previous track" on:click=move|_|ui.command(PlayerCommand::Previous)><Icon name="previous"/></button>
      <button class="play-button" aria-label=move||if ui.playback.with(|p|p.playing){"Pause"}else{"Play"} on:click=move|_|ui.command(PlayerCommand::Toggle)>
        {move||view!{<Icon name=if ui.playback.with(|p|p.playing){"pause"}else{"play"}/>}}
      </button>
      <button class="icon-button" aria-label="Next track" on:click=move|_|ui.command(PlayerCommand::Next)><Icon name="next"/></button>
      <button class="icon-button subtle" aria-label="Cycle repeat mode" on:click=move|_|ui.command(PlayerCommand::Repeat)><Icon name="repeat"/></button>
    </div>
    <div class="timeline"><time>{move||time(ui.playback.with(|p|p.position))}</time><PlaybackSlider ui seek=true/><time>{move||time(ui.playback.with(|p|p.duration))}</time></div></div>
    <div class="transport-extra"><button class="icon-button" class:liked=move||ui.playback.with(|p|p.liked) aria-label="Like or unlike track" on:click=move|_|ui.command(PlayerCommand::Like(!ui.playback.get_untracked().liked))><Icon name="heart"/></button><button class="icon-button video-toggle" class:active=move||ui.route.get()=="video" aria-label="Toggle video" aria-pressed=move||(ui.route.get()=="video").to_string() title=move||if ui.route.get()=="video"{"Hide video"}else{"Show video"} on:click=move|_|{if ui.route.get_untracked()=="video"{ui.navigate("player")}else{ui.show_player("video")}}><Icon name="video"/></button><button class="icon-button volume-toggle" class:muted=move||ui.playback.with(|p|p.muted) aria-label=move||if ui.playback.with(|p|p.muted){"Unmute"}else{"Mute"} aria-pressed=move||ui.playback.with(|p|p.muted).to_string() title=move||if ui.playback.with(|p|p.muted){"Unmute"}else{"Mute"} on:click=move|_|ui.command(PlayerCommand::Mute)><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m3 9 5 0 5-4v14l-5-4H3z"/><g class="sound-waves"><path d="M17 8a6 6 0 0 1 0 8M20 5a10 10 0 0 1 0 14"/></g><path class="mute-slash" pathLength="1" d="m17 9 5 6m0-6-5 6"/></svg></button><PlaybackSlider ui seek=false/></div></footer>}
}
#[component]
fn EmbeddedPlayer(ui: Ui) -> impl IntoView {
    view! {<section class="embedded-page"><Show when=move||ui.route.get()=="signin"><div class="page-heading"><h1>"Google sign-in"</h1><button class="secondary" on:click=move|_|ui.navigate("account")>"Back"</button></div></Show><div id="player-surface"/></section>}
}
#[component]
fn AccountView(ui: Ui) -> impl IntoView {
    view! {<section class="account-page"><svg class="account-logo" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 4h3.5v12.5h7.3V20H5z" fill="currentColor"/><path d="m12 5 8 4.7-8 4.7z" fill="currentColor" opacity="0.65"/></svg><h1>"YouTube Music"</h1><Show when=move||ui.playback.with(|p|p.signed_in) fallback=move||view!{<p>"Sign in to access your library, playlists, and recommendations."</p><button class="primary google-signin" on:click=move|_|ui.navigate("signin")><strong>"G"</strong>"Sign in with Google"</button><small>"Your session is stored on this device."</small>}><p>"Your account is connected."</p><div class="actions"><button class="primary" on:click=move|_|ui.navigate("library")>"Open library"</button><button class="secondary" on:click=move|_|spawn_local(async move{ui.message(call::<()>("logout",json!({})).await);ui.page.set(CatalogPage::default());ui.home_library.set(CatalogPage::default());})>"Sign out"</button></div></Show></section>}
}
#[component]
fn PlayerView(ui: Ui) -> impl IntoView {
    ui.load_lyrics();
    view! {
      <Show when=move||!ui.track.with(|t|t.id.is_empty()) fallback=||view!{<section class="empty-player"><Icon name="music"/><h1>"Nothing playing"</h1><p>"Search for music to start listening."</p></section>}>
        <section class="player-page">
        <div class="page-heading"><h1>"Now Playing"</h1></div>
        <Show when=move||ui.playback.with(|p|p.error.is_some())><p class="error" role="alert">{move||ui.playback.with(|p|p.error.clone())}</p></Show>
        <div class="player-grid"><div class="player-main">{move||ui.settings.with(|s|s.theme.layout.main.clone()).into_iter().map(|block|view!{<PlayerBlock ui block/>}).collect_view()}</div><aside class="player-side">{move||ui.settings.with(|s|s.theme.layout.side.clone()).into_iter().map(|block|view!{<PlayerBlock ui block/>}).collect_view()}</aside></div></section>
      </Show>
    }
}
#[component]
fn PlayerBlock(ui: Ui, block: Block) -> impl IntoView {
    let queue_limit = RwSignal::new(50usize);
    match block{
        Block::Artwork=>view!{<div class="now-artwork">{move||view!{<Cover url=ui.track.get().artwork large=true/>}}<h2>{move||{let t=ui.track.get().title;if t.is_empty(){"Nothing playing".into()}else{t}}}</h2><p><ArtistLink ui/></p></div>}.into_any(),
        Block::Queue=>view!{<section class="player-block"><div class="section-heading"><h2>"Queue"</h2><span>{move||format!("{} tracks",ui.queue.get().len())}</span></div><div class="queue-list">{move||ui.queue.get().into_iter().enumerate().take(queue_limit.get()).map(|(i,t)|{let id=t.id.clone();view!{<div class="queue-row" class:current=move||ui.track.get().id==id><span class="track-number">{i+1}</span><button class="queue-track" on:click=move|e|{if !text_selected(&e){ui.command(PlayerCommand::Play(t.id.clone()))}}><Cover url=t.artwork/><span><strong>{t.title}</strong><small>{t.artist}</small></span></button><button class="text-button queue-move" aria-label="Move track up" disabled=i==0 on:click=move|_|ui.command(PlayerCommand::Move{from:i,to:i.saturating_sub(1)})>"Up"</button><button class="icon-button" aria-label="Remove from queue" on:click=move|_|ui.command(PlayerCommand::Remove(i))><Icon name="close"/></button></div>}}).collect_view()}</div><Show when=move||{ui.queue.with(Vec::len)>queue_limit.get()}><button class="text-button" on:click=move|_|queue_limit.update(|n|*n+=50)>"Show more"</button></Show><Show when=move||ui.queue.get().is_empty()><p class="muted">"Play a track to start your queue."</p></Show></section>}.into_any(),
        Block::Lyrics=>view!{<LyricsBlock ui/>}.into_any(),
        Block::Details=>view!{<section class="player-block"><h2>"Track details"</h2><dl><dt>"Artist"</dt><dd>{move||ui.track.get().artist}</dd><dt>"Duration"</dt><dd>{move||time(ui.playback.with(|p|p.duration))}</dd><dt>"Source"</dt><dd>"YouTube Music"</dd></dl></section>}.into_any(),
    }
}
#[component]
fn LyricsBlock(ui: Ui) -> impl IntoView {
    let list = NodeRef::<html::Div>::new();
    let active = Memo::new(move |_| {
        ui.lyrics.with(|l| {
            active_lyric(
                &l.lines,
                ui.playback.with(|p| p.position) - ui.settings.with(|s| s.plugins.lyrics_offset),
            )
        })
    });
    Effect::new(move || {
        let index = active.get();
        if let (Some(index), Some(container)) = (index, list.get()) {
            set_timeout(
                move || {
                    let Some(line) = container
                        .query_selector(&format!("#lyric-{index}"))
                        .ok()
                        .flatten()
                    else {
                        return;
                    };
                    let rect = line.get_bounding_client_rect();
                    let parent = container.get_bounding_client_rect();
                    let options = web_sys::ScrollToOptions::new();
                    options.set_top(
                        (container.scroll_top() as f64 + rect.y()
                            - parent.y()
                            - parent.height() / 2.0
                            + rect.height() / 2.0)
                            .max(0.0),
                    );
                    let reduced = web_sys::window()
                        .and_then(|w| {
                            w.match_media("(prefers-reduced-motion: reduce)")
                                .ok()
                                .flatten()
                        })
                        .is_some_and(|m| m.matches());
                    options.set_behavior(if reduced {
                        web_sys::ScrollBehavior::Instant
                    } else {
                        web_sys::ScrollBehavior::Smooth
                    });
                    container.scroll_to_with_scroll_to_options(&options);
                },
                Duration::from_millis(16),
            );
        }
    });
    view! {<section class="player-block lyrics"><div class="section-heading"><h2>"Lyrics"</h2><button class="text-button" on:click=move|_|{ui.lyric_id.set(String::new());ui.load_lyrics();}>"Refresh"</button></div><p class="lyric-source">{move||ui.lyrics.with(|l|l.source.clone())}</p><div class="lyric-lines" node_ref=list tabindex="0" aria-label="Lyrics">{move||{
        let lyrics=ui.lyrics.get();
        if lyrics.lines.is_empty(){view!{<p class="plain-lyrics">{if lyrics.plain.is_empty(){"No lyrics available.".into()}else{lyrics.plain}}</p>}.into_any()}
        else{lyrics.lines.into_iter().enumerate().map(|(i,line)|view!{<button id=format!("lyric-{i}") class:active=move||active.get()==Some(i) class:near=move||active.get().is_some_and(|a|a.abs_diff(i)<=1) aria-current=move||if active.get()==Some(i){"true"}else{"false"} on:click=move|e|{if !text_selected(&e){ui.command(PlayerCommand::Seek{position:(line.time+ui.settings.with_untracked(|s|s.plugins.lyrics_offset)).max(0.0),track_id:ui.track.with_untracked(|t|t.id.clone())})}}>{line.text}</button>}).collect_view().into_any()}
    }}</div></section>}
}
fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
