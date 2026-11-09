An early preview of lazytune, a Rust desktop client for YouTube Music.

- Search, albums, playlists, artist pages and following, queue, and account sign-in.
- Synchronized lyrics, video inside the app, artwork colors, and customizable layout.
- Native media controls, Discord Rich Presence, Last.fm, ListenBrainz, and SponsorBlock.
- Windows taskbar hover controls for liking, previous, play/pause, and next.
- Portable Windows executable; Apple Silicon and Intel macOS DMGs; Linux AppImage, DEB, and RPM.

### Installation

Choose the file for your operating system. `arm64` is Apple Silicon; `x64` is Intel/AMD.

- **Windows 10/11 (x64):** run the EXE. Microsoft Edge WebView2 Runtime is required.
- **macOS 14+ (Apple Silicon or Intel):** open the DMG and drag lazytune to Applications. These preview bundles are ad-hoc signed, not Apple notarized. macOS may block the first launch; use Privacy & Security → Open Anyway only after verifying the download and checksum.
- **Linux (x64):** DEB targets Ubuntu 22.04+ / compatible distributions; RPM needs WebKitGTK 4.1 and GStreamer codecs. For AppImage, make the file executable before launching. AppImage needs FUSE 2, or run with `--appimage-extract-and-run`. Playback depends on the desktop's available codecs and graphics drivers.

SHA-256 checksums are supplied per platform. Windows executables are not Authenticode signed and may trigger SmartScreen warnings. macOS notarization and Windows signing are pending.

### Preview limitations

Windows sign-in, playback, seeking, and volume persistence have been exercised locally. Automated builds do not establish macOS/Linux sign-in, audio playback, media keys, suspend/resume, or long background-playback compatibility. Those hardware checks and live scrobbling verification are still required before a stable release.

YouTube Music availability, account restrictions, and subscription rules still apply. lazytune is an independent client and is not affiliated with Google or YouTube.
