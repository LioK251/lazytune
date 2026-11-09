use crate::app::{self, AppState};
use lazytune_model::PlayerCommand;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};
use std::{sync::Mutex, time::Duration};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
struct Controls(Mutex<MediaControls>);
pub fn setup(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Open lazytune", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Play / pause", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", "Next track", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &next, &quit])?;
    let tray = TrayIconBuilder::new()
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/icon.png"
        ))?)
        .tooltip("lazytune")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                app::focus(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => app::focus(app),
            "quit" => app.exit(0),
            "toggle" | "next" => {
                let command = if event.id.as_ref() == "next" {
                    PlayerCommand::Next
                } else {
                    PlayerCommand::Toggle
                };
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = app::control(&app, command).await;
                });
            }
            _ => {}
        })
        .build(app);
    app.state::<AppState>()
        .tray_available
        .store(tray.is_ok(), std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(target_os = "windows"))]
    let hwnd = None;
    #[cfg(target_os = "windows")]
    let hwnd = app
        .get_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0);
    match MediaControls::new(PlatformConfig {
        dbus_name: "lazytune",
        display_name: "lazytune",
        hwnd,
    }) {
        Ok(mut controls) => {
            let handle = app.clone();
            controls.attach(move |event| {
                let command = match event {
                    MediaControlEvent::Play => Some(PlayerCommand::Resume),
                    MediaControlEvent::Pause | MediaControlEvent::Stop => {
                        Some(PlayerCommand::Pause)
                    }
                    MediaControlEvent::Toggle => Some(PlayerCommand::Toggle),
                    MediaControlEvent::Next => Some(PlayerCommand::Next),
                    MediaControlEvent::Previous => Some(PlayerCommand::Previous),
                    MediaControlEvent::SetPosition(p) => Some(PlayerCommand::Seek {
                        position: p.0.as_secs_f64(),
                        track_id: handle
                            .state::<AppState>()
                            .playback
                            .lock()
                            .unwrap()
                            .track
                            .id
                            .clone(),
                    }),
                    MediaControlEvent::SetVolume(v) => {
                        Some(PlayerCommand::Volume(v.clamp(0.0, 1.0)))
                    }
                    MediaControlEvent::Raise => {
                        app::focus(&handle);
                        None
                    }
                    MediaControlEvent::Quit => {
                        handle.exit(0);
                        None
                    }
                    _ => None,
                };
                if let Some(command) = command {
                    let app = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = app::control(&app, command).await;
                    });
                }
            })?;
            app.manage(Controls(Mutex::new(controls)));
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let mut last = String::new();
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let p = handle.state::<AppState>().playback.lock().unwrap().clone();
                    let changed = last != p.track.id;
                    last = p.track.id.clone();
                    let app = handle.clone();
                    let _ = handle.run_on_main_thread(move || {
                        let state = app.state::<Controls>();
                        let Ok(mut c) = state.0.lock() else { return };
                        if changed {
                            let _ = c.set_metadata(MediaMetadata {
                                title: Some(&p.track.title),
                                artist: Some(&p.track.artist),
                                album: Some(&p.track.album),
                                cover_url: if p.track.artwork.is_empty() {
                                    None
                                } else {
                                    Some(&p.track.artwork)
                                },
                                duration: Some(Duration::from_secs_f64(p.duration)),
                            });
                        }
                        let progress = Some(MediaPosition(Duration::from_secs_f64(p.position)));
                        let _ = c.set_playback(if p.track.id.is_empty() {
                            MediaPlayback::Stopped
                        } else if p.playing {
                            MediaPlayback::Playing { progress }
                        } else {
                            MediaPlayback::Paused { progress }
                        });
                    });
                }
            });
        }
        Err(_) => {
            app.state::<AppState>().status(
                "media",
                "unavailable",
                "OS media controls are unavailable in this desktop session",
            );
        }
    }
    Ok(())
}
