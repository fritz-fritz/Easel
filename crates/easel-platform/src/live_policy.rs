// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Power, lock, and full-screen sensors that drive live-playback pause policy.
//!
//! [`PlaybackClock::pause`] / [`PlaybackClock::resume`] are the sole timeline
//! controls; hosts map sensor snapshots through [`pause_reason_for`] before
//! mutating the shared clock.

#[cfg(any(target_os = "linux", test))]
use std::fs;
#[cfg(test)]
use std::path::Path;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;

use easel_core::PlaybackPolicy;

/// Snapshot of session conditions that may freeze live decode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LivePolicySensors {
    /// True when the system is drawing power from a battery (not AC).
    pub on_battery: bool,
    /// True when a full-screen application is considered active.
    pub full_screen_app: bool,
    /// True when the session is locked (screensaver / lock screen).
    pub session_locked: bool,
    /// True when the machine is about to sleep or recently resumed mid-suspend.
    pub suspended: bool,
}

/// Stable pause reason string written into live IPC and diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LivePauseReason {
    /// Profile requests pause while on battery.
    Battery,
    /// Profile requests pause while a full-screen app is active.
    FullScreen,
    /// Session lock always pauses decode (no profile opt-out).
    SessionLock,
    /// Suspend / sleep always pauses decode.
    Suspend,
}

impl LivePauseReason {
    /// Human-readable token for IPC / status lines.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Battery => "battery",
            Self::FullScreen => "full_screen",
            Self::SessionLock => "session_lock",
            Self::Suspend => "suspend",
        }
    }
}

/// Returns the highest-priority pause reason for `policy` + `sensors`, if any.
///
/// Priority: suspend → session lock → battery → full-screen.
#[must_use]
pub fn pause_reason_for(
    policy: &PlaybackPolicy,
    sensors: &LivePolicySensors,
) -> Option<LivePauseReason> {
    if sensors.suspended {
        return Some(LivePauseReason::Suspend);
    }
    if sensors.session_locked {
        return Some(LivePauseReason::SessionLock);
    }
    if policy.pause_on_battery && sensors.on_battery {
        return Some(LivePauseReason::Battery);
    }
    if policy.pause_for_full_screen_app && sensors.full_screen_app {
        return Some(LivePauseReason::FullScreen);
    }
    None
}

/// Probes the current OS session for live pause sensors.
///
/// Missing sensors default to "not active" rather than inventing a pause. Full-screen
/// detection is best-effort and may remain false when no stable public API exists.
#[must_use]
pub fn probe_live_policy_sensors() -> LivePolicySensors {
    LivePolicySensors {
        on_battery: probe_on_battery(),
        full_screen_app: probe_full_screen_app(),
        session_locked: probe_session_locked(),
        suspended: false,
    }
}

fn probe_on_battery() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux_on_battery()
    }
    #[cfg(windows)]
    {
        windows_on_battery()
    }
    #[cfg(target_os = "macos")]
    {
        macos_on_battery()
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        false
    }
}

fn probe_session_locked() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux_session_locked()
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        // Feasibility spikes: no validated public lock sensor wired yet.
        false
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        false
    }
}

fn probe_full_screen_app() -> bool {
    // No portable, trustworthy full-screen probe yet (X11/_NET_WM_STATE is
    // session-specific; Wayland lacks a universal API). Leave false until a
    // backend-specific sensor lands; policy wiring is still exercised via tests.
    false
}

#[cfg(target_os = "linux")]
fn linux_on_battery() -> bool {
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return false;
    };
    let mut saw_battery = false;
    let mut any_discharging = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let type_path = path.join("type");
        let Ok(kind) = fs::read_to_string(&type_path) else {
            continue;
        };
        if !kind.trim().eq_ignore_ascii_case("Battery") {
            continue;
        }
        saw_battery = true;
        let status_path = path.join("status");
        if let Ok(status) = fs::read_to_string(&status_path) {
            let status = status.trim();
            if status.eq_ignore_ascii_case("Discharging") {
                any_discharging = true;
            }
        }
    }
    saw_battery && any_discharging
}

#[cfg(target_os = "linux")]
fn linux_session_locked() -> bool {
    // Prefer loginctl for the current session; ignore failures (SSH, containers).
    let output = Command::new("loginctl")
        .args(["show-session", "self", "-p", "LockedHint", "--value"])
        .output();
    match output {
        Ok(result) if result.status.success() => {
            let value = String::from_utf8_lossy(&result.stdout);
            value.trim().eq_ignore_ascii_case("yes")
        }
        _ => false,
    }
}

#[cfg(windows)]
fn windows_on_battery() -> bool {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: SYSTEM_POWER_STATUS is a plain POD out-parameter for GetSystemPowerStatus.
    let ok = unsafe { GetSystemPowerStatus(&raw mut status) };
    if ok.is_err() {
        return false;
    }
    // ACLineStatus: 0 = offline (battery), 1 = online, 255 = unknown.
    status.ACLineStatus == 0
}

#[cfg(target_os = "macos")]
fn macos_on_battery() -> bool {
    let output = Command::new("pmset").args(["-g", "batt"]).output();
    match output {
        Ok(result) if result.status.success() => {
            let text = String::from_utf8_lossy(&result.stdout).to_lowercase();
            text.contains("discharging")
        }
        _ => false,
    }
}

/// Test helper: parses a synthetic sysfs battery tree under `root`.
#[cfg(test)]
#[must_use]
pub fn linux_on_battery_from_sysfs(root: &Path) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    let mut saw_battery = false;
    let mut any_discharging = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = fs::read_to_string(path.join("type")) else {
            continue;
        };
        if !kind.trim().eq_ignore_ascii_case("Battery") {
            continue;
        }
        saw_battery = true;
        if let Ok(status) = fs::read_to_string(path.join("status"))
            && status.trim().eq_ignore_ascii_case("Discharging")
        {
            any_discharging = true;
        }
    }
    saw_battery && any_discharging
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{LoopMode, PlaybackPolicy};

    fn policy(battery: bool, full_screen: bool) -> PlaybackPolicy {
        PlaybackPolicy {
            loop_mode: LoopMode::Loop,
            rate: 1.0,
            maximum_frames_per_second: Some(30),
            pause_on_battery: battery,
            pause_for_full_screen_app: full_screen,
        }
    }

    #[test]
    fn suspend_outranks_other_reasons() {
        let sensors = LivePolicySensors {
            on_battery: true,
            full_screen_app: true,
            session_locked: true,
            suspended: true,
        };
        assert_eq!(
            pause_reason_for(&policy(true, true), &sensors),
            Some(LivePauseReason::Suspend)
        );
    }

    #[test]
    fn session_lock_pauses_without_profile_flag() {
        let sensors = LivePolicySensors {
            session_locked: true,
            ..LivePolicySensors::default()
        };
        assert_eq!(
            pause_reason_for(&policy(false, false), &sensors),
            Some(LivePauseReason::SessionLock)
        );
    }

    #[test]
    fn battery_respects_policy_flag() {
        let sensors = LivePolicySensors {
            on_battery: true,
            ..LivePolicySensors::default()
        };
        assert_eq!(
            pause_reason_for(&policy(true, false), &sensors),
            Some(LivePauseReason::Battery)
        );
        assert_eq!(pause_reason_for(&policy(false, false), &sensors), None);
    }

    #[test]
    fn full_screen_respects_policy_flag() {
        let sensors = LivePolicySensors {
            full_screen_app: true,
            ..LivePolicySensors::default()
        };
        assert_eq!(
            pause_reason_for(&policy(false, true), &sensors),
            Some(LivePauseReason::FullScreen)
        );
        assert_eq!(pause_reason_for(&policy(false, false), &sensors), None);
    }

    #[test]
    fn sysfs_battery_discharging_detection() {
        let root = std::env::temp_dir().join(format!(
            "easel-battery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let bat = root.join("BAT0");
        fs::create_dir_all(&bat).unwrap();
        fs::write(bat.join("type"), "Battery\n").unwrap();
        fs::write(bat.join("status"), "Discharging\n").unwrap();
        assert!(linux_on_battery_from_sysfs(&root));
        fs::write(bat.join("status"), "Charging\n").unwrap();
        assert!(!linux_on_battery_from_sysfs(&root));
        let _ = fs::remove_dir_all(root);
    }
}
