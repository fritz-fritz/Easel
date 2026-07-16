// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Windows still wallpaper backend using `IDesktopWallpaper`.

#![allow(unsafe_code)]

use std::path::Path;

use windows::Win32::Foundation::CO_E_ALREADYINITIALIZED;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Shell::{DWPOS_FILL, DesktopWallpaper, IDesktopWallpaper};
use windows::core::PCWSTR;

use crate::{
    BackendCapabilities, BackendError, DisplayWallpaper, WallpaperBackend, WallpaperOutput,
};

/// Windows per-monitor still wallpaper backend.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsDesktopBackend;

impl WallpaperBackend for WindowsDesktopBackend {
    fn id(&self) -> &'static str {
        "windows-idesktopwallpaper"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            per_display_images: true,
            virtual_desktop_image: true,
            activities: false,
            workspaces: false,
            lock_screen: false,
        }
    }

    fn apply(&self, output: &WallpaperOutput) -> Result<(), BackendError> {
        match output {
            WallpaperOutput::PerDisplay(displays) => {
                for wallpaper in displays {
                    self.validate_output_path(&wallpaper.path)?;
                }
                apply_per_display(displays)
            }
            WallpaperOutput::VirtualDesktop(path) => {
                self.validate_output_path(path)?;
                apply_virtual_desktop(path)
            }
        }
    }
}

fn apply_per_display(displays: &[DisplayWallpaper]) -> Result<(), BackendError> {
    let _com = ComGuard::new()?;
    let wallpaper = create_desktop_wallpaper()?;
    let monitors = enumerate_monitors(&wallpaper)?;

    for item in displays {
        let monitor = monitors
            .iter()
            .find(|monitor| rects_match(monitor.rect, item.logical_rect))
            .ok_or_else(|| {
                BackendError::Platform(format!(
                    "no Windows monitor matched geometry {}x{}+{}+{}",
                    item.logical_rect.width,
                    item.logical_rect.height,
                    item.logical_rect.x,
                    item.logical_rect.y
                ))
            })?;
        set_wallpaper_for_monitor(&wallpaper, &monitor.device_path, &item.path)?;
    }
    Ok(())
}

fn apply_virtual_desktop(path: &Path) -> Result<(), BackendError> {
    let _com = ComGuard::new()?;
    let wallpaper = create_desktop_wallpaper()?;
    let wide = wide_path(path)?;
    unsafe {
        wallpaper
            .SetWallpaper(PCWSTR::null(), PCWSTR(wide.as_ptr()))
            .map_err(|error| BackendError::Platform(format!("SetWallpaper failed: {error}")))?;
        wallpaper
            .SetPosition(DWPOS_FILL)
            .map_err(|error| BackendError::Platform(format!("SetPosition failed: {error}")))?;
    }
    Ok(())
}

fn create_desktop_wallpaper() -> Result<IDesktopWallpaper, BackendError> {
    unsafe {
        CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)
            .map_err(|error| BackendError::Platform(format!("CoCreateInstance failed: {error}")))
    }
}

fn set_wallpaper_for_monitor(
    wallpaper: &IDesktopWallpaper,
    monitor_id: &str,
    path: &Path,
) -> Result<(), BackendError> {
    let monitor_wide: Vec<u16> = monitor_id
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let path_wide = wide_path(path)?;
    unsafe {
        wallpaper
            .SetWallpaper(PCWSTR(monitor_wide.as_ptr()), PCWSTR(path_wide.as_ptr()))
            .map_err(|error| BackendError::Platform(format!("SetWallpaper failed: {error}")))?;
        wallpaper
            .SetPosition(DWPOS_FILL)
            .map_err(|error| BackendError::Platform(format!("SetPosition failed: {error}")))?;
    }
    Ok(())
}

fn wide_path(path: &Path) -> Result<Vec<u16>, BackendError> {
    let Some(text) = path.to_str() else {
        return Err(BackendError::Platform(format!(
            "wallpaper path is not valid UTF-8: {}",
            path.display()
        )));
    };
    Ok(text.encode_utf16().chain(std::iter::once(0)).collect())
}

#[derive(Debug)]
struct MonitorInfo {
    device_path: String,
    rect: easel_core::LogicalRect,
}

fn enumerate_monitors(wallpaper: &IDesktopWallpaper) -> Result<Vec<MonitorInfo>, BackendError> {
    let count = unsafe {
        wallpaper.GetMonitorDevicePathCount().map_err(|error| {
            BackendError::Platform(format!("GetMonitorDevicePathCount failed: {error}"))
        })?
    };

    let mut monitors = Vec::with_capacity(count as usize);
    for index in 0..count {
        let path_hstring = unsafe {
            wallpaper.GetMonitorDevicePathAt(index).map_err(|error| {
                BackendError::Platform(format!("GetMonitorDevicePathAt failed: {error}"))
            })?
        };
        let device_path = unsafe {
            path_hstring.to_string().map_err(|error| {
                BackendError::Platform(format!("monitor device path is not valid UTF-16: {error}"))
            })?
        };
        let rect = unsafe {
            wallpaper
                .GetMonitorRECT(PCWSTR(path_hstring.as_ptr()))
                .map_err(|error| {
                    BackendError::Platform(format!("GetMonitorRECT failed: {error}"))
                })?
        };
        monitors.push(MonitorInfo {
            device_path,
            rect: easel_core::LogicalRect {
                x: rect.left,
                y: rect.top,
                width: u32::try_from((rect.right - rect.left).max(0)).unwrap_or(0),
                height: u32::try_from((rect.bottom - rect.top).max(0)).unwrap_or(0),
            },
        });
    }
    Ok(monitors)
}

fn rects_match(left: easel_core::LogicalRect, right: easel_core::LogicalRect) -> bool {
    left.x == right.x
        && left.y == right.y
        && left.width == right.width
        && left.height == right.height
}

struct ComGuard {
    should_uninit: bool,
}

impl ComGuard {
    fn new() -> Result<Self, BackendError> {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result.is_ok() {
            Ok(Self {
                should_uninit: true,
            })
        } else if result == CO_E_ALREADYINITIALIZED {
            Ok(Self {
                should_uninit: false,
            })
        } else {
            Err(BackendError::Platform(format!(
                "CoInitializeEx failed: {result:?}"
            )))
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.should_uninit {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

/// Builds the ordered wallpaper assignment list for tests without touching COM.
#[cfg(test)]
pub fn plan_monitor_assignments(
    monitors: &[(String, easel_core::LogicalRect)],
    displays: &[DisplayWallpaper],
) -> Result<Vec<(String, std::path::PathBuf)>, BackendError> {
    let mut planned = Vec::with_capacity(displays.len());
    for item in displays {
        let monitor = monitors
            .iter()
            .find(|(_, rect)| rects_match(*rect, item.logical_rect))
            .ok_or_else(|| BackendError::Platform("geometry mismatch".into()))?;
        planned.push((monitor.0.clone(), item.path.clone()));
    }
    Ok(planned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use easel_core::{DisplayId, LogicalRect};

    #[test]
    fn assignment_plan_matches_geometry() {
        let monitors = vec![
            (
                r"\\.\DISPLAY1".into(),
                LogicalRect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
            ),
            (
                r"\\.\DISPLAY2".into(),
                LogicalRect {
                    x: 1920,
                    y: 0,
                    width: 2560,
                    height: 1440,
                },
            ),
        ];
        let displays = vec![DisplayWallpaper {
            display_id: DisplayId::from_u128(1),
            path: PathBuf::from(r"C:\easel\right.png"),
            logical_rect: LogicalRect {
                x: 1920,
                y: 0,
                width: 2560,
                height: 1440,
            },
        }];
        let planned = plan_monitor_assignments(&monitors, &displays).expect("plan");
        assert_eq!(planned[0].0, r"\\.\DISPLAY2");
        assert_eq!(planned[0].1, PathBuf::from(r"C:\easel\right.png"));
    }
}
