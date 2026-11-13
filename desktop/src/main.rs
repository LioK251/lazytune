#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod account;
mod app;
mod media;
mod plugins;
mod storage;
#[cfg(target_os = "windows")]
mod taskbar;
mod update;
mod visualizer;
use tauri::Manager;
fn main() {
    let context = tauri::generate_context!();
    #[cfg(debug_assertions)]
    let context = {
        let mut context = context;
        if std::env::var_os("LAZYTUNE_INSPECT").is_some() {
            context.config_mut().app.windows[0].additional_browser_args = Some(
                "--remote-debugging-port=9222 --autoplay-policy=no-user-gesture-required".into(),
            );
        }
        context
    };
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            app::focus(app)
        }))
        .invoke_handler(tauri::generate_handler![
            app::bootstrap,
            app::window_action,
            app::catalog,
            app::player_command,
            app::player_report,
            app::save_settings,
            app::import_theme,
            app::export_theme,
            app::apply_theme,
            app::choose_wallpaper,
            app::reset_theme,
            app::show_player,
            app::logout,
            app::lyrics,
            app::artwork_colors,
            app::set_credential,
            app::lastfm_connect,
            app::lastfm_finish,
            app::plugin_status,
            app::undo_skip,
            update::check_update,
            update::acknowledge_update,
            update::open_update,
            visualizer::visualizer_active
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            eprintln!("lazytune: initializing state");
            let state =
                app::AppState::new(app.path().app_data_dir()?).map_err(std::io::Error::other)?;
            let size = state.settings.lock().unwrap().window_size;
            app.manage(state);
            #[cfg(target_os = "windows")]
            if let Err(e) = taskbar::setup(app.handle()) {
                app.state::<app::AppState>()
                    .status("taskbar", "unavailable", &e.to_string());
            }
            if let Some(w) = app.get_window("main") {
                #[cfg(not(target_os = "macos"))]
                w.set_decorations(false)?;
                let _ = w.set_size(tauri::LogicalSize::new(
                    size[0].clamp(640, 3840),
                    size[1].clamp(480, 2160),
                ));
            }
            let player_app = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(e) = app::create_player(&player_app) {
                    *player_app.state::<app::AppState>().warning.lock().unwrap() = Some(e);
                }
            });
            #[cfg(debug_assertions)]
            eprintln!("lazytune: initializing desktop controls");
            media::setup(app.handle())?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { plugins::run(handle).await });
            #[cfg(debug_assertions)]
            eprintln!("lazytune: ready");
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            let state = window.state::<app::AppState>();
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if state.settings.lock().unwrap().close_to_tray
                        && state
                            .tray_available
                            .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        api.prevent_close();
                        let _ = window.hide();
                        visualizer::sync(window.app_handle());
                    } else {
                        window.app_handle().exit(0);
                    }
                }
                tauri::WindowEvent::Resized(size) => {
                    if let Ok(scale) = window.scale_factor() {
                        let logical = size.to_logical::<u32>(scale);
                        state.settings.lock().unwrap().window_size =
                            [logical.width, logical.height];
                    }
                    visualizer::sync(window.app_handle());
                }
                tauri::WindowEvent::Focused(_) => visualizer::sync(window.app_handle()),
                _ => {}
            }
        })
        .build(context);
    match result {
        Ok(app) => app.run(|handle, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let s = handle.state::<app::AppState>();
                if s.preserve_settings
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    return;
                }
                let _ = storage::atomic_json(
                    &s.directory.join("settings.json"),
                    &*s.settings.lock().unwrap(),
                );
            }
        }),
        Err(e) => {
            rfd::MessageDialog::new().set_title("lazytune could not start").set_description(format!("{e}\n\nWindows needs Microsoft Edge WebView2 Runtime. Linux needs WebKitGTK 4.1 and the GStreamer codecs listed in README.md.")).set_level(rfd::MessageLevel::Error).show();
        }
    }
}
