fn main() {
    for name in [
        "LAZYTUNE_DISCORD_ID",
        "LAZYTUNE_LASTFM_KEY",
        "LAZYTUNE_LASTFM_SECRET",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "bootstrap",
            "window_action",
            "catalog",
            "player_command",
            "player_report",
            "save_settings",
            "import_theme",
            "export_theme",
            "apply_theme",
            "choose_wallpaper",
            "reset_theme",
            "show_player",
            "logout",
            "lyrics",
            "artwork_colors",
            "set_credential",
            "lastfm_connect",
            "lastfm_finish",
            "plugin_status",
            "undo_skip",
        ]),
    ))
    .expect("Tauri build configuration");
}
