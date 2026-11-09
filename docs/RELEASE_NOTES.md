Fixes the “YouTube Music is not optimized for your browser” message that blocked account sign-in on macOS.

- The macOS player now sends Safari browser identification, so YouTube Music loads instead of rejecting the embedded WebKit browser.
- The version shown in Settings and sent to integrations now follows the packaged app version.

### Installation

Choose the file for your operating system. `arm64` is Apple Silicon; `x64` is Intel/AMD.

- **Windows 10/11 (x64):** run the EXE. Microsoft Edge WebView2 Runtime is required.
- **macOS 14+ (Apple Silicon or Intel):** open the DMG and drag lazytune to Applications. These bundles are ad-hoc signed, not Apple notarized. macOS may block the first launch; use Privacy & Security → Open Anyway only after verifying the download and checksum.
- **Linux (x64):** DEB targets Ubuntu 22.04+ / compatible distributions; RPM needs WebKitGTK 4.1 and GStreamer codecs. For AppImage, make the file executable before launching. AppImage needs FUSE 2, or run with `--appimage-extract-and-run`. Playback depends on the desktop's available codecs and graphics drivers.

SHA-256 checksums are supplied per platform. Windows executables are not Authenticode signed and may trigger SmartScreen warnings. macOS notarization and Windows signing are pending.

### Known limitations

The original browser rejection, corrected YouTube Music page, and Google sign-in form were checked in WebKit 27.2. Windows sign-in, playback, seeking, and volume persistence have been exercised locally. Completing account authorization on a Mac, native macOS/Linux playback, media keys, suspend/resume, long background playback, and live scrobbling still need hardware verification.

YouTube Music availability, account restrictions, and subscription rules still apply. lazytune is an independent client and is not affiliated with Google or YouTube.
