# Dynamic and live wallpaper plan

## Capability definitions

Wallspan uses precise terms in its code, interface, and support matrix:

| Capability | Behavior | Runtime requirement |
| --- | --- | --- |
| Static | One rendered still remains active until replaced. | Public still-wallpaper backend. |
| Dynamic stills | A schedule, solar rule, or time-of-day timeline atomically replaces still frames. | Scheduler plus public still-wallpaper backend. |
| Animated image | A GIF, animated WebP, or similar local container plays continuously. | Decoder, compositor, and persistent desktop surface. |
| Video | A silent local video plays continuously. | Video decoder, compositor, and persistent desktop surface. |

Native vendor-specific “dynamic wallpaper” bundles are not the portable domain format. Importers
may translate a supported bundle into a Wallspan time-of-day timeline, while original files and
metadata remain intact.

## Feasibility assessment

Dynamic stills are feasible on any backend that can already apply a still image. They reuse the
renderer and scheduler, can be prepared ahead of a transition, and have straightforward failure
behavior.

Animated images and video are also feasible, but not through one uniform OS wallpaper call. Qt
Multimedia supplies playback state and `VideoOutput`; a live-wallpaper implementation still needs
a platform adapter that owns persistent surfaces at the desktop background layer.

| Platform/session | Dynamic stills | Animated/video host | Initial position |
| --- | --- | --- | --- |
| KDE Plasma 6 | Static backend applies each frame. | Documented QML wallpaper plugin model. | First supported live target. |
| Other Linux desktops | Static settings backend applies each frame. | Desktop/compositor-specific; no universal Wayland attachment. | Probe individually; poster fallback. |
| Windows | `IDesktopWallpaper` applies still files. | Public wallpaper API does not expose video playback. | Feasibility spike; experimental if safe. |
| macOS | AppKit applies a still image per screen. | Public `setDesktopImageURL` contract is still-image oriented. | Feasibility spike; experimental if safe. |

The application must never advertise a live capability based only on the operating-system name.
It probes the current session and decoder, reports evidence in diagnostics, and falls back to the
poster frame when no validated live host exists.

## Live session design

One logical player owns timing for a display group. Each output surface consumes the same frame
and applies its own crop and physical-layout transform. This preserves continuity across bezels
and avoids the drift caused by independent per-monitor players.

```mermaid
flowchart TD
    Source["Local media source"] --> Decoder["Qt Multimedia decoder"]
    Decoder --> Clock["Shared media clock"]
    Clock --> Compositor["Per-display crops + transforms"]
    Compositor --> Host["Capability-checked desktop host"]
    Poster["Rendered poster frame"] --> Host
```

The session lifecycle is `prepare → poster → play ↔ pause → stop`. Prepare validates the local
source, decoder, poster, surfaces, and policy without removing the current wallpaper. Playback
starts only after every requested surface is ready. A partial multi-monitor start is a failure.

## Media and policy defaults

- Local files only for the initial motion implementation.
- Audio tracks are detected for diagnostics and always discarded.
- Loop playback and a 30 fps ceiling by default.
- Pause on battery and while a full-screen application is active by default.
- Pause on session lock and suspend; revalidate display topology and host surfaces on resume.
- Prefer hardware decoding when available, with measured software fallback rather than an
  unconditional guarantee.
- Extract or render a poster frame before Apply becomes available.
- Surface codec/container failures in the UI; do not silently transcode user media.

Streaming URLs are out of scope. They introduce network continuity, authentication, buffering,
cache, content changes, and provider-policy concerns that are independent of local playback.

## Delivery gates

A live backend moves from experimental to supported only after it demonstrates:

1. stable ownership below desktop icons across login, shell restart, workspace changes, and OS
   updates;
2. synchronized display crops within one presented frame;
3. bounded CPU, GPU, memory, and battery use on representative hardware;
4. correct pause/resume behavior for power, lock, sleep, and full-screen policy;
5. deterministic poster fallback after decoder, compositor, or host failure;
6. clear diagnostics for unavailable codecs and hardware acceleration.

## Primary references

- Qt Multimedia video overview: https://doc.qt.io/qt-6/videooverview.html
- Qt Quick `MediaPlayer`: https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html
- KDE Plasma extension development: https://develop.kde.org/docs/plasma/
- Windows `IDesktopWallpaper`: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-idesktopwallpaper
- Windows `SetWallpaper`: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-idesktopwallpaper-setwallpaper
- macOS `setDesktopImageURL`: https://developer.apple.com/documentation/appkit/nsworkspace/setdesktopimageurl%28_%3Afor%3Aoptions%3A%29
