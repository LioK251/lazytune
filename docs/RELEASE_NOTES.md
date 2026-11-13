Fixes playback startup, unexpected stops, missing search results, and macOS account sign-in.

- Play starts a selected track without toggling video. Early navigation waits for the player to be ready.
- Unexpected stops get bounded recovery, including in the background. Pause, Stop, and ended tracks are respected.
- Temporary metadata updates no longer change the playback state or mix song timelines.
- Search includes YouTube Music's top-result cards, including the exact result for `цветы темный принц`.
- macOS Google sign-in uses a separate view inside the app and preserves the existing account profile.
- Now Playing artwork is smaller and responds to window height, keeping the title, artist, and lyrics visible.
- Appearance includes optional background pulse and spectrum bars driven by the music audio. Both are off by default.
- An update notice appears once per newer stable release. It opens the release page; updates are installed manually.
- Loading text has been removed; errors and retry actions remain.

### Installation

Choose the file for your operating system. `arm64` is Apple Silicon; `x64` is Intel/AMD.

- **Windows 10/11 (x64):** run the EXE. Microsoft Edge WebView2 Runtime is required.
- **macOS 14+ (Apple Silicon or Intel):** open the DMG and drag lazytune to Applications. These bundles are ad-hoc signed, not Apple notarized. macOS may block the first launch; use Privacy & Security → Open Anyway only after verifying the download and checksum.
- **Linux (x64):** DEB targets Ubuntu 22.04+ / compatible distributions; RPM needs WebKitGTK 4.1 and GStreamer codecs. For AppImage, make the file executable before launching. AppImage needs FUSE 2, or run with `--appimage-extract-and-run`. Playback depends on the desktop's available codecs and graphics drivers.

SHA-256 checksums are supplied per platform. Windows executables are not Authenticode signed and may trigger SmartScreen warnings. macOS notarization and Windows signing are pending.

Install 0.1.2 manually if you use an earlier version; older versions do not contain the update checker. Settings, volume, themes, and account sessions are preserved. Visualization requires working Web Audio support in the platform's browser engine.

YouTube Music availability, account restrictions, and subscription rules still apply. lazytune is an independent client and is not affiliated with Google or YouTube.
