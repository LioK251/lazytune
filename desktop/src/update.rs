use crate::{app, storage};
use lazytune_model::UpdateInfo;
use serde::Deserialize;
use std::{path::Path, sync::Mutex};
use tauri::{AppHandle, Manager, Webview};

const REPOSITORY: &str = "https://github.com/LioK251/lazytune";
const LATEST: &str = "https://api.github.com/repos/LioK251/lazytune/releases/latest";
const NOTIFIED: &str = "notified-update.json";
static CHECKED: Mutex<bool> = Mutex::new(false);
static OFFERED: Mutex<Option<UpdateInfo>> = Mutex::new(None);

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
}

fn version(value: &str) -> Option<[u64; 3]> {
    if value.len() > 40 {
        return None;
    }
    let mut parts = value.strip_prefix('v').unwrap_or(value).split('.');
    let mut numbers = [0; 3];
    for number in &mut numbers {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *number = part.parse().ok()?;
        if number.to_string() != part {
            return None;
        }
    }
    parts.next().is_none().then_some(numbers)
}

fn candidate(release: Release, current: &str, notified: Option<&str>) -> Option<UpdateInfo> {
    let next = version(&release.tag_name)?;
    if release.draft
        || release.prerelease
        || next <= version(current)?
        || release.html_url != format!("{REPOSITORY}/releases/tag/{}", release.tag_name)
        || notified.and_then(version).is_some_and(|seen| seen >= next)
    {
        return None;
    }
    Some(UpdateInfo {
        version: format!("{}.{}.{}", next[0], next[1], next[2]),
        release_url: release.html_url,
    })
}

fn last_notified(directory: &Path) -> Option<String> {
    storage::read_limited(&directory.join(NOTIFIED), 128)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<String>(&bytes).ok())
        .filter(|value| version(value).is_some())
}

async fn fetch(app: &AppHandle) -> Option<UpdateInfo> {
    let state = app.state::<app::AppState>();
    let response = state
        .client
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;
    candidate(
        serde_json::from_value(crate::plugins::json_response(response).await.ok()?).ok()?,
        env!("CARGO_PKG_VERSION"),
        last_notified(&state.directory).as_deref(),
    )
}

#[tauri::command]
pub async fn check_update(window: Webview, app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    app::local(&window)?;
    {
        let mut checked = CHECKED.lock().map_err(|_| "Update state unavailable")?;
        if *checked {
            return Ok(None);
        }
        *checked = true;
    }
    let info = fetch(&app).await;
    *OFFERED.lock().map_err(|_| "Update state unavailable")? = info.clone();
    Ok(info)
}

#[tauri::command]
pub fn acknowledge_update(window: Webview, app: AppHandle, version: String) -> Result<(), String> {
    app::local(&window)?;
    let offered = OFFERED.lock().map_err(|_| "Update state unavailable")?;
    if offered.as_ref().is_none_or(|info| info.version != version) {
        return Err("Unknown update".into());
    }
    storage::atomic_json(
        &app.state::<app::AppState>().directory.join(NOTIFIED),
        &version,
    )
}

#[tauri::command]
pub fn open_update(window: Webview, version: String) -> Result<(), String> {
    app::local(&window)?;
    let offered = OFFERED.lock().map_err(|_| "Update state unavailable")?;
    let info = offered
        .as_ref()
        .filter(|info| info.version == version)
        .ok_or("Unknown update")?;
    open::that(&info.release_url).map_err(|_| "Could not open the release page".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str) -> Release {
        Release {
            tag_name: tag.into(),
            html_url: format!("{REPOSITORY}/releases/tag/{tag}"),
            draft: false,
            prerelease: false,
        }
    }
    #[test]
    fn stable_version_order_and_notification_history() {
        assert!(candidate(release("v0.1.2"), "0.1.1", None).is_some());
        assert!(candidate(release("v0.1.2"), "0.1.2", None).is_none());
        assert!(candidate(release("v0.1.2"), "0.1.3", None).is_none());
        assert!(candidate(release("v0.1.10"), "0.1.9", Some("0.1.2")).is_some());
        assert!(candidate(release("v0.1.2"), "0.1.1", Some("0.1.2")).is_none());
        for tag in ["v0.1.2-beta", "v0.1", "v01.1.2", "../0.1.2", "v0.1.2.3"] {
            assert!(candidate(release(tag), "0.1.1", None).is_none());
        }
    }
    #[test]
    fn rejects_unpublished_and_foreign_release_links() {
        let mut next = release("v0.1.2");
        next.prerelease = true;
        assert!(candidate(next, "0.1.1", None).is_none());
        let mut next = release("v0.1.2");
        next.draft = true;
        assert!(candidate(next, "0.1.1", None).is_none());
        for url in [
            "https://github.com.evil.test/LioK251/lazytune/releases/tag/v0.1.2",
            "file:///tmp/update",
            "https://github.com/LioK251/other/releases/tag/v0.1.2",
        ] {
            let mut next = release("v0.1.2");
            next.html_url = url.into();
            assert!(candidate(next, "0.1.1", None).is_none());
        }
    }
    #[test]
    fn notification_survives_settings_saves_and_damaged_state() {
        let dir = tempfile::tempdir().unwrap();
        storage::atomic_json(&dir.path().join(NOTIFIED), &"0.1.2").unwrap();
        storage::atomic_json(
            &dir.path().join("settings.json"),
            &lazytune_model::Settings::default(),
        )
        .unwrap();
        assert_eq!(last_notified(dir.path()).as_deref(), Some("0.1.2"));
        std::fs::write(dir.path().join(NOTIFIED), b"bad").unwrap();
        assert!(last_notified(dir.path()).is_none());
    }
}
