use url::Url;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub const WEBKIT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";

pub fn navigation_allowed(url: &Url) -> bool {
    url.as_str() == "about:blank"
        || (url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port_or_known_default() == Some(443)
            && matches!(
                url.host_str(),
                Some(
                    "music.youtube.com"
                        | "accounts.google.com"
                        | "accounts.youtube.com"
                        | "www.youtube.com"
                        | "consent.youtube.com"
                        | "consent.google.com"
                )
            ))
}

pub fn login_url() -> Url {
    let mut next = Url::parse("https://www.youtube.com/signin").unwrap();
    next.query_pairs_mut()
        .append_pair("action_handle_signin", "true")
        .append_pair("app", "desktop")
        .append_pair("next", "https://music.youtube.com/");
    let mut login = Url::parse("https://accounts.google.com/ServiceLogin").unwrap();
    login
        .query_pairs_mut()
        .append_pair("service", "youtube")
        .append_pair("continue", next.as_str());
    login
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use crate::app::{self, AppState};
    use lazytune_model::SurfaceBounds;
    use serde_json::json;
    use std::{
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Mutex,
        },
        time::Duration,
    };
    use tauri::{webview::NewWindowResponse, AppHandle, Emitter, Manager};

    pub const PROFILE: [u8; 16] = *b"lazytune-player1";

    #[derive(Default)]
    pub struct State {
        generation: AtomicU64,
        returning: AtomicBool,
        transition: Mutex<()>,
    }

    fn current(app: &AppHandle, generation: u64) -> bool {
        app.state::<AppState>()
            .account
            .generation
            .load(Ordering::SeqCst)
            == generation
    }

    pub fn close(app: &AppHandle) -> Result<(), String> {
        let state = app.state::<AppState>();
        let transition = state.account.transition.lock().unwrap();
        // A confirmed return may still be restoring the player's song after the UI leaves sign-in.
        if !state.account.returning.load(Ordering::SeqCst) {
            state.account.generation.fetch_add(1, Ordering::SeqCst);
        }
        drop(transition);
        if let Some(view) = app.get_webview("account") {
            view.close().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn cancel(app: &AppHandle) -> Result<(), String> {
        let state = app.state::<AppState>();
        let transition = state.account.transition.lock().unwrap();
        state.account.returning.store(false, Ordering::SeqCst);
        state.account.generation.fetch_add(1, Ordering::SeqCst);
        drop(transition);
        if let Some(view) = app.get_webview("account") {
            view.close().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn check_document(app: AppHandle, generation: u64) {
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            if !current(&app, generation)
                || app
                    .state::<AppState>()
                    .account
                    .returning
                    .load(Ordering::SeqCst)
            {
                return;
            }
            if let Some(view) = app.get_webview("account") {
                let callback_app = app.clone();
                if view
                    .eval_with_callback(
                        "!!document.body && !!document.body.querySelector('input,form,button,a') && document.body.getBoundingClientRect().height > 0",
                        move |result| {
                            if current(&callback_app, generation)
                                && !callback_app.state::<AppState>().account.returning.load(Ordering::SeqCst)
                                && serde_json::from_str::<bool>(&result).ok() != Some(true)
                            {
                                let _ = callback_app.emit_to("main", "notice", "Google sign-in could not load. Try again.");
                            }
                        },
                    )
                    .is_err()
                {
                    let _ = app.emit_to("main", "notice", "Google sign-in could not load. Try again.");
                }
            }
        });
    }

    fn returned(app: AppHandle, generation: u64) {
        let state = app.state::<AppState>();
        let transition = state.account.transition.lock().unwrap();
        if !current(&app, generation) || state.account.returning.swap(true, Ordering::SeqCst) {
            return;
        }
        let control_revision = state.control_revision.load(Ordering::SeqCst);
        let resume = {
            let mut playback = state.playback.lock().unwrap();
            let resume = playback.clone();
            playback.ready = false;
            playback.signed_in = false;
            resume
        };
        if let Some(view) = app.get_webview("account") {
            let _ = view.close();
        }
        let result = app
            .get_webview("player")
            .ok_or("Player is unavailable")
            .and_then(|view| {
                view.navigate(Url::parse("https://music.youtube.com/").unwrap())
                    .map_err(|_| "Could not refresh the player after sign-in")
            });
        drop(transition);
        drop(state);
        if let Err(error) = result {
            app.state::<AppState>()
                .account
                .returning
                .store(false, Ordering::SeqCst);
            let _ = app.emit_to("main", "notice", error);
            return;
        }
        tauri::async_runtime::spawn(async move {
            let result = async {
                for _ in 0..134 {
                    if !current(&app, generation) {
                        return Ok(());
                    }
                    let ready = app.state::<AppState>().playback.lock().unwrap().clone();
                    if ready.ready && ready.signed_in {
                        if !resume.track.id.is_empty()
                            && app
                                .state::<AppState>()
                                .control_revision
                                .load(Ordering::SeqCst)
                                == control_revision
                        {
                            app::request(
                                &app,
                                "restore",
                                json!({
                                    "track_id": resume.track.id,
                                    "position": resume.position,
                                    "playing": resume.playing || resume.play_requested,
                                    "control_revision": control_revision,
                                }),
                            )
                            .await?;
                        }
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
                Err("Google sign-in did not complete. Try again.".to_string())
            }
            .await;
            if current(&app, generation) {
                app.state::<AppState>()
                    .account
                    .returning
                    .store(false, Ordering::SeqCst);
                if let Err(error) = result {
                    let _ = app.emit_to("main", "notice", error);
                }
            }
        });
    }

    pub fn show(app: &AppHandle, bounds: &SurfaceBounds, retry: bool) -> Result<(), String> {
        if !retry
            && app
                .state::<AppState>()
                .account
                .returning
                .load(Ordering::SeqCst)
        {
            return Ok(());
        }
        if retry {
            let state = app.state::<AppState>();
            let transition = state.account.transition.lock().unwrap();
            state.account.generation.fetch_add(1, Ordering::SeqCst);
            state.account.returning.store(false, Ordering::SeqCst);
            drop(transition);
            if let Some(view) = app.get_webview("account") {
                view.close().map_err(|e| e.to_string())?;
            }
        }
        if let Some(view) = app.get_webview("account") {
            view.set_bounds(tauri::Rect {
                position: tauri::LogicalPosition::new(bounds.x, bounds.y).into(),
                size: tauri::LogicalSize::new(bounds.width, bounds.height).into(),
            })
            .map_err(|e| e.to_string())?;
            return view.show().map_err(|e| e.to_string());
        }
        let generation = {
            let state = app.state::<AppState>();
            let _transition = state.account.transition.lock().unwrap();
            if state.account.returning.load(Ordering::SeqCst) {
                return Ok(());
            }
            state.account.generation.fetch_add(1, Ordering::SeqCst) + 1
        };
        let page_app = app.clone();
        // Authentication uses the same cookies, with a fresh view and no playback adapter or IPC capability.
        let builder = tauri::webview::WebviewBuilder::new(
            "account",
            tauri::WebviewUrl::External(login_url()),
        )
        .data_store_identifier(PROFILE)
        .user_agent(WEBKIT_USER_AGENT)
        .on_navigation(navigation_allowed)
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_page_load(move |_, page| {
            if page.event() == tauri::webview::PageLoadEvent::Finished
                && page.url().origin().ascii_serialization() == "https://music.youtube.com"
            {
                returned(page_app.clone(), generation);
            }
        });
        let parent = app.get_window("main").ok_or("Main window unavailable")?;
        let view = parent
            .add_child(
                builder,
                tauri::LogicalPosition::new(bounds.x, bounds.y),
                tauri::LogicalSize::new(bounds.width, bounds.height),
            )
            .map_err(|e| e.to_string())?;
        view.show().map_err(|e| e.to_string())?;
        check_document(app.clone(), generation);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub use native::{cancel, close, show, State, PROFILE};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_authentication_frames_are_allowed() {
        for destination in [
            "https://accounts.google.com/v3/signin/identifier",
            "https://accounts.youtube.com/accounts/CheckConnection",
            "https://consent.youtube.com/",
            "https://music.youtube.com/?authuser=0",
            "about:blank",
        ] {
            assert!(
                navigation_allowed(&Url::parse(destination).unwrap()),
                "{destination}"
            );
        }
    }

    #[test]
    fn authentication_navigation_does_not_widen_remote_trust() {
        for destination in [
            "http://accounts.google.com/",
            "https://accounts.google.com.evil.example/",
            "https://evil.accounts.google.com/",
            "https://accounts.google.com:444/",
            "https://user:password@accounts.google.com/",
            "about:blank#redirect",
            "data:text/html,test",
            "javascript:alert(1)",
            "file:///tmp/index.html",
        ] {
            assert!(
                !navigation_allowed(&Url::parse(destination).unwrap()),
                "{destination}"
            );
        }
    }

    #[test]
    fn google_login_continues_through_youtube_to_music() {
        let login = login_url();
        assert_eq!(login.host_str(), Some("accounts.google.com"));
        let pairs: std::collections::HashMap<_, _> = login.query_pairs().into_owned().collect();
        assert_eq!(pairs.get("service").map(String::as_str), Some("youtube"));
        let continuation = Url::parse(&pairs["continue"]).unwrap();
        assert_eq!(continuation.host_str(), Some("www.youtube.com"));
        assert!(continuation
            .query_pairs()
            .any(|(key, value)| key == "next" && value == "https://music.youtube.com/"));
    }
}
