# ADR 0003: Dynamic stills and live media use separate platform paths

- Status: accepted
- Date: 2026-07-15

## Context

“Dynamic wallpaper” covers two materially different behaviors. A time-of-day or solar sequence
periodically replaces one still image with another. Animated images and video require continuous
decoding plus a persistent surface attached to the desktop. Public wallpaper APIs on Windows and
macOS accept image files; Qt Multimedia can decode and render media but does not attach a window
to an operating system's desktop wallpaper layer.

## Decision

Model three presentation modes: static, dynamic stills, and live media. Dynamic stills use the
existing scheduler, raster renderer, and static `WallpaperBackend`. Live media uses a separate
`LiveWallpaperBackend` whose capabilities are probed for the active desktop session.

The live path has these invariants:

- one logical media clock synchronizes all participating displays;
- source audio is never routed;
- every source has a rendered poster frame for startup and fallback;
- power, full-screen application, sleep, and session-lock policies can pause decoding;
- unsupported or failed live integration degrades explicitly to the poster frame;
- codec/container support is reported from the runtime decoder, not promised globally.

Qt Multimedia is the preferred cross-platform decoding and preview layer. Each platform still
requires a native host adapter. KDE Plasma can use a purpose-built QML wallpaper plugin. Windows
and macOS require feasibility spikes and support tiers because their public still-wallpaper APIs
do not provide video playback.

## Consequences

- Scheduled stills can ship before live playback and remain broadly portable.
- Video is a planned capability, not an implied feature of every backend.
- Platform support documentation must distinguish static, dynamic-still, animated-image, and
  video capabilities.
- Live-media tests include synchronization drift, power use, codec failure, and host recovery.

## References

- https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html
- https://doc.qt.io/qt-6/videooverview.html
- https://develop.kde.org/docs/plasma/
- https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-idesktopwallpaper
- https://developer.apple.com/documentation/appkit/nsworkspace/setdesktopimageurl%28_%3Afor%3Aoptions%3A%29
