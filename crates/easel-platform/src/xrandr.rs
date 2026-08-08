// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! XRandR monitor enumeration for Linux still-wallpaper backends.

use std::process::Command;

use easel_core::LogicalRect;

use crate::{BackendError, DisplayWallpaper};

/// One XRandR logical monitor (`xrandr --listmonitors`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XrandrMonitor {
    /// Connector / logical monitor name (for example `DP-1`).
    pub name: String,
    /// Logical compositor rectangle in pixels.
    pub rect: LogicalRect,
}

/// Parses `xrandr --listmonitors` stdout into named geometries.
///
/// Accepts both plain (`640x360+0+180`) and millimetre-annotated
/// (`640/600x360/340+0+180`) size forms.
#[must_use]
pub fn parse_list_monitors(stdout: &str) -> Vec<XrandrMonitor> {
    let mut monitors = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Monitors:") {
            continue;
        }
        // "0: +VNC-0 1920/508x1200/317+0+0  VNC-0" (leading +/* mark primary)
        // "0: DP-1 640/600x360/340+0+180  VNC-0" or "1: DP-2 768x432+640+0"
        let Some((_index, rest)) = trimmed.split_once(':') else {
            continue;
        };
        let rest = rest.trim();
        let mut parts = rest.split_whitespace();
        let Some(raw_name) = parts.next() else {
            continue;
        };
        // xrandr marks the primary monitor with a leading '+' (and occasionally '*').
        let name = raw_name
            .strip_prefix('+')
            .or_else(|| raw_name.strip_prefix('*'))
            .unwrap_or(raw_name);
        if name.is_empty() {
            continue;
        }
        let Some(geom) = parts.next() else {
            continue;
        };
        if let Some(rect) = parse_monitor_geometry(geom) {
            monitors.push(XrandrMonitor {
                name: name.to_owned(),
                rect,
            });
        }
    }
    monitors
}

/// Runs `xrandr --listmonitors` on the current `DISPLAY`.
pub fn list_monitors() -> Result<Vec<XrandrMonitor>, BackendError> {
    let output = Command::new("xrandr")
        .arg("--listmonitors")
        .output()
        .map_err(|error| BackendError::Platform(format!("xrandr failed: {error}")))?;
    if !output.status.success() {
        return Err(BackendError::Platform(format!(
            "xrandr --listmonitors exited with {}",
            output.status
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let monitors = parse_list_monitors(&stdout);
    if monitors.is_empty() {
        return Err(BackendError::Platform(
            "xrandr --listmonitors returned no monitors".into(),
        ));
    }
    Ok(monitors)
}

/// Near-exact logical-rect equality used to match Easel crops to platform outputs.
///
/// Allows a 1px slop on each edge: Qt screen probes occasionally disagree with
/// `xrandr --listmonitors` by a single pixel on this VNC host (for example
/// `640x1201` vs `640x1200`), which would otherwise fail the whole Apply.
#[must_use]
pub fn rects_match(left: LogicalRect, right: LogicalRect) -> bool {
    const TOLERANCE: i32 = 1;
    i32::abs(left.x - right.x) <= TOLERANCE
        && i32::abs(left.y - right.y) <= TOLERANCE
        && i32::abs(i32::try_from(left.width).unwrap_or(i32::MAX) - i32::try_from(right.width).unwrap_or(i32::MAX))
            <= TOLERANCE
        && i32::abs(
            i32::try_from(left.height).unwrap_or(i32::MAX) - i32::try_from(right.height).unwrap_or(i32::MAX),
        ) <= TOLERANCE
}


/// Plans `(monitor_name, path)` assignments by matching wallpaper geometry to XRandR.
pub fn plan_monitor_assignments(
    monitors: &[XrandrMonitor],
    displays: &[DisplayWallpaper],
) -> Result<Vec<(String, std::path::PathBuf)>, BackendError> {
    let mut planned = Vec::with_capacity(displays.len());
    for item in displays {
        let monitor = monitors
            .iter()
            .find(|monitor| rects_match(monitor.rect, item.logical_rect))
            .ok_or_else(|| {
                BackendError::Platform(format!(
                    "no XRandR monitor matched geometry {}x{}+{}+{}",
                    item.logical_rect.width,
                    item.logical_rect.height,
                    item.logical_rect.x,
                    item.logical_rect.y
                ))
            })?;
        planned.push((monitor.name.clone(), item.path.clone()));
    }
    Ok(planned)
}

/// Orders wallpaper paths to match `monitors` list order (required by feh Xinerama).
pub fn paths_in_monitor_order(
    monitors: &[XrandrMonitor],
    displays: &[DisplayWallpaper],
) -> Result<Vec<std::path::PathBuf>, BackendError> {
    let assignments = plan_monitor_assignments(monitors, displays)?;
    let mut paths = Vec::with_capacity(monitors.len());
    for monitor in monitors {
        let path = assignments
            .iter()
            .find(|(name, _)| name == &monitor.name)
            .map(|(_, path)| path.clone())
            .ok_or_else(|| {
                BackendError::Platform(format!(
                    "no wallpaper planned for XRandR monitor {}",
                    monitor.name
                ))
            })?;
        paths.push(path);
    }
    Ok(paths)
}

fn parse_monitor_geometry(geom: &str) -> Option<LogicalRect> {
    // width[/wmm]xheight[/hmm]+x+y
    let (size, position) = geom.split_once('+')?;
    let (width_part, height_part) = size.split_once('x')?;
    let width = parse_dim(width_part)?;
    let height = parse_dim(height_part)?;
    let mut coords = position.split('+');
    let x: i32 = coords.next()?.parse().ok()?;
    let y: i32 = coords.next()?.parse().ok()?;
    Some(LogicalRect {
        x,
        y,
        width,
        height,
    })
}

fn parse_dim(part: &str) -> Option<u32> {
    part.split('/').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::DisplayId;
    use std::path::PathBuf;

    #[test]
    fn parses_list_monitors_with_millimetres() {
        let stdout = "\
Monitors: 3
 0: DP-1 640/600x360/340+0+180  VNC-0
 1: DP-2 768/700x432/400+640+0 
 2: DP-3 512/530x288/300+1408+180 
";
        let monitors = parse_list_monitors(stdout);
        assert_eq!(monitors.len(), 3);
        assert_eq!(monitors[0].name, "DP-1");
        assert_eq!(
            monitors[0].rect,
            LogicalRect {
                x: 0,
                y: 180,
                width: 640,
                height: 360
            }
        );
        assert_eq!(monitors[1].name, "DP-2");
        assert_eq!(
            monitors[1].rect,
            LogicalRect {
                x: 640,
                y: 0,
                width: 768,
                height: 432
            }
        );
        assert_eq!(monitors[2].name, "DP-3");
        assert_eq!(
            monitors[2].rect,
            LogicalRect {
                x: 1408,
                y: 180,
                width: 512,
                height: 288
            }
        );
    }

    #[test]
    fn parses_plain_geometry() {
        let stdout = "Monitors: 1\n 0: eDP-1 1920x1080+0+0\n";
        let monitors = parse_list_monitors(stdout);
        assert_eq!(monitors.len(), 1);
        assert_eq!(
            monitors[0].rect,
            LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
    }

    #[test]
    fn strips_primary_marker_from_monitor_name() {
        // xrandr --listmonitors marks the primary with a leading '+'.
        let stdout = "Monitors: 1\n 0: +VNC-0 1920/508x1200/317+0+0  VNC-0\n";
        let monitors = parse_list_monitors(stdout);
        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].name, "VNC-0");
        assert_eq!(
            monitors[0].rect,
            LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1200
            }
        );
    }

    #[test]
    fn rects_match_allows_one_pixel_slop() {
        let monitor = LogicalRect {
            x: 0,
            y: 0,
            width: 640,
            height: 1200,
        };
        let qt_probe = LogicalRect {
            x: 0,
            y: 0,
            width: 640,
            height: 1201,
        };
        assert!(rects_match(monitor, qt_probe));
        let elsewhere = LogicalRect {
            x: 640,
            y: 0,
            width: 640,
            height: 1200,
        };
        assert!(!rects_match(monitor, elsewhere));
    }

    #[test]
    fn plans_assignments_and_monitor_order() {
        let monitors =
            parse_list_monitors("Monitors: 2\n 0: DP-1 640x360+0+0\n 1: DP-2 800x600+640+0\n");
        let displays = vec![
            DisplayWallpaper {
                display_id: DisplayId::from_u128(2),
                path: PathBuf::from("/tmp/b.png"),
                logical_rect: LogicalRect {
                    x: 640,
                    y: 0,
                    width: 800,
                    height: 600,
                },
            },
            DisplayWallpaper {
                display_id: DisplayId::from_u128(1),
                path: PathBuf::from("/tmp/a.png"),
                logical_rect: LogicalRect {
                    x: 0,
                    y: 0,
                    width: 640,
                    height: 360,
                },
            },
        ];
        let planned = plan_monitor_assignments(&monitors, &displays).expect("plan");
        assert_eq!(planned.len(), 2);
        let ordered = paths_in_monitor_order(&monitors, &displays).expect("order");
        assert_eq!(
            ordered,
            vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")]
        );
    }

    #[test]
    fn rejects_geometry_mismatch() {
        let monitors = parse_list_monitors("Monitors: 1\n 0: DP-1 640x360+0+0\n");
        let displays = vec![DisplayWallpaper {
            display_id: DisplayId::from_u128(1),
            path: PathBuf::from("/tmp/a.png"),
            logical_rect: LogicalRect {
                x: 10,
                y: 10,
                width: 640,
                height: 360,
            },
        }];
        assert!(plan_monitor_assignments(&monitors, &displays).is_err());
    }
}
