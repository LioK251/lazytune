# lazytune

A small Rust desktop client for YouTube Music. Windows, macOS, and Linux use their system webview instead of bundling Electron or Chromium.

The app includes search, recommendations, a queue, transport controls, video inside the main window, account sign-in, native media controls, and a tray menu. The interface is written in Rust with Leptos/WASM. A bundled JavaScript adapter connects it to the YouTube Music player.

Built-in integrations:

- Discord Rich Presence: **Listening to Music**, with track, artist, artwork, and progress.
- Ad filtering and SponsorBlock with selectable categories.
- Synchronized lyrics from LRCLIB.
- Last.fm and ListenBrainz scrobbling with a local retry queue.

These are independent settings, not downloaded executable plugins. Third-party plugin loading and full feature parity with Pear Desktop are not implemented.

## Run and build

Windows needs Microsoft Edge WebView2 Runtime. The release executable is portable; settings and the web session stay in the current user's application-data directory. The executable does not include the webview runtime.

macOS requires 14 or newer. Linux requires WebKitGTK 4.1, GStreamer codecs, a D-Bus session, and a Secret Service provider for account tokens. A Linux binary still needs these system libraries.

Install Rust 1.91.1 or newer, Python 3.11 or newer, and the native build tools for your platform. On Windows use the MSVC toolchain and Visual Studio C++ Build Tools; on macOS install Xcode Command Line Tools.

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
python scripts/build.py --release
```

The output is `target/release/lazytune.exe` on Windows and `target/release/lazytune` on macOS/Linux. `python scripts/build.py` creates a debug build. Frontend assets are embedded in the executable; `dist` is not needed beside it.

Ubuntu build dependencies:

```sh
sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libsecret-1-dev libasound2-dev patchelf
```

For playback also install `gstreamer1.0-plugins-good`, `gstreamer1.0-plugins-bad`, and `gstreamer1.0-libav`. Codec availability varies by distribution.

## Accounts and credentials

Use **Account → Sign in with Google**. Google's own sign-in page runs in an embedded view; lazytune does not receive your password. Some accounts or webview platforms may be rejected by Google. Account authorization and platform compatibility must be checked before a public release.

The default Discord application ID is configured in `desktop/src/plugins.rs`. A build can override it with `LAZYTUNE_DISCORD_ID`.

For Last.fm, provide `LAZYTUNE_LASTFM_KEY` and `LAZYTUNE_LASTFM_SECRET` at build time. The build script also reads a local `.env_lastfm` file containing tab-separated `API key` and `Shared secret` lines. That file is ignored by Git. Build logs do not print its values. Do not commit it or include it in artifacts. As with other desktop Last.fm clients, an application secret compiled into a binary is extractable; user session tokens remain in the OS credential store.

Enable Last.fm, click **Authorize**, approve access in your browser, and click **Finish connection**. For ListenBrainz, save your user token in Integrations. User credentials use Windows Credential Manager, macOS Keychain, or Linux Secret Service. Turning an integration off stops submissions. Unsent listens are kept locally for up to seven days and are associated with the account that created them.

## Appearance

The base palette is monochrome. **Colors from artwork**, **Use album artwork as background**, and **Glassy playback bar** are enabled for new profiles and theme resets. Discord Rich Presence and ad blocking are also enabled for new profiles. Existing saved choices are preserved. Appearance changes save automatically after a short pause. The sidebar can collapse to icons. Windows/Linux use custom window controls; macOS retains native window chrome. Cover colors form a slowly moving gradient. Motion respects the operating system's reduced-motion preference. Scrollbars are hidden; mouse-wheel and keyboard scrolling remain available.

Player blocks can move between the main area and the side panel, change order, or be hidden. Themes export appearance only.

Each appearance effect and integration can be turned off independently. **Automatically select music videos** remains off by default; with it off, closing video returns to audio mode. Playlist and album cards have their own Play buttons, and the playing artist's name opens their page. Older imported themes with omitted optional appearance fields retain their previous behavior.

Artist pages include Follow/Following controls. Volume is saved automatically and restored at startup. Seek commands are bound to the current track and are unavailable during track loading.

`.lazytune-theme` files are strict JSON; see [the schema](themes/schema.json). Imports are previewed before applying. Unknown fields, scripts, CSS, HTML, SVG, URLs, file paths, and integration settings are rejected. A theme can embed one static PNG/JPEG. Limits: 16 MiB per theme, 10 MiB per image, 4096 × 4096 pixels, and 96 MiB decoder allocation. Images are decoded and re-encoded before display. Settings and themes are written atomically.

## Checks

```sh
cargo fmt --all --check
node --test scripts/player.test.cjs
cargo test -p lazytune-model -p lazytune --locked
cargo clippy -p lazytune-model -p lazytune --locked -- -D warnings
cargo clippy -p lazytune-ui --target wasm32-unknown-unknown --locked -- -D warnings
```

Build the frontend before checking the desktop crate on a clean checkout. [Build and package](.github/workflows/build.yml) checks Windows x64, Linux x64, Apple Silicon, and Intel Macs. Each successful platform job uploads installable packages and SHA-256 checksums. Packaging self-checks run with `python scripts/package_test.py`.

The browser interaction checks use the built `dist` with a simulated desktop bridge. Install Playwright with `npm install --prefix .tools/qa playwright`, then run `node --test scripts/ui.test.cjs`. They use installed Microsoft Edge by default; set `PLAYWRIGHT_CHANNEL=chrome` to use Chrome. These checks cover delayed slider updates, command failures, glass controls, browse backgrounds, and video toggling without accessing your account.

On Windows, set `LAZYTUNE_INSPECT=1` before starting a **debug** build to enable local inspection ports 9222 (UI) and 9223 (player). Do not use an inspection build for everyday account access. Release builds do not enable these ports.

## Packaging and release checks

Install Tauri CLI 2 (`npm install --global @tauri-apps/cli@2.12.4`). After the release build, run `tauri bundle --bundles dmg` from `desktop` on macOS, or `tauri bundle --bundles deb,rpm,appimage` on Linux. Windows ships the standalone executable. These packages use the same embedded assets. Run `python scripts/package.py` from the repository root to collect versioned files in `artifacts/package/<platform>-<architecture>/`.

The workflow builds Linux on Ubuntu 22.04 and includes GStreamer in AppImage packages. macOS builds target version 14 or newer and preserve native window controls. Preview DMGs use ad-hoc signatures; that does not replace a Developer ID signature or notarization. Before local packaging, install `cargo-about` 0.9.3 and generate the bundled notices with `cargo about generate --workspace --locked --fail docs/licenses.hbs --output-file artifacts/THIRD_PARTY_LICENSES.html` (create `artifacts/` first). CI does this automatically.

The dependency audit blocks security advisories. Specific upstream maintenance notices for Leptos/GTK build macros and the zbus 5 stack are documented in `deny.toml`; revisit those exceptions when the parent libraries migrate.

To prepare a release, synchronize the version in `Cargo.toml` and `desktop/tauri.conf.json`, update [release notes](docs/RELEASE_NOTES.md), and run **Actions → Build and package → Run workflow** with **draft_release** enabled. All platform and dependency checks must pass before the workflow creates a draft prerelease. It verifies the downloaded checksums before uploading. Publication is a separate action after reviewing the draft.

Public release gates: run the platform CI, verify Google sign-in and playback on real hardware, verify media keys and suspend/resume, and run a 30-minute hidden-playback test. The Windows development machine alone cannot establish macOS/Linux compatibility.

For distribution, sign the Windows executable with Authenticode before calculating checksums. On macOS supply `APPLE_SIGNING_IDENTITY` and the notarization credentials supported by Tauri (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, or App Store Connect API credentials). Verify with `codesign --verify --deep --strict` and `spctl --assess`. Signing certificates and notarization access are not included in this repository.

YouTube Music's internal page APIs can change. Ad filtering is best effort, and service availability, regional restrictions, subscription rules, and platform codecs still apply. This is an independent client, not an official YouTube product.

## Icon and license

Use [the transparent 1024 px PNG](desktop/icons/lazytune-1024.png) for the Discord application profile. The SVG, Windows ICO, and macOS ICNS are in the same directory. Regenerate them with `python scripts/icons.py` (requires Pillow).

MIT. See [third-party notices](THIRD_PARTY_NOTICES.md).
