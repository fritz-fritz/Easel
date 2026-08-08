// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Active live-wallpaper session held by the desktop process.

use std::sync::Mutex;

use easel_platform::LiveWallpaperSession;

static LIVE_SESSION: Mutex<Option<Box<dyn LiveWallpaperSession>>> = Mutex::new(None);

/// Replaces any running live session with `session`.
pub fn replace_live_session(session: Box<dyn LiveWallpaperSession>) -> Result<(), String> {
    let mut guard = LIVE_SESSION
        .lock()
        .map_err(|_| "live session mutex poisoned".to_owned())?;
    if let Some(previous) = guard.take() {
        let _ = previous.stop();
    }
    *guard = Some(session);
    Ok(())
}

/// Stops and clears the active live session, if any.
pub fn stop_live_session() -> Result<(), String> {
    let mut guard = LIVE_SESSION
        .lock()
        .map_err(|_| "live session mutex poisoned".to_owned())?;
    if let Some(previous) = guard.take() {
        previous.stop().map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Pauses the active live session clock (policy / tray).
pub fn pause_live_session() -> Result<(), String> {
    let mut guard = LIVE_SESSION
        .lock()
        .map_err(|_| "live session mutex poisoned".to_owned())?;
    if let Some(session) = guard.as_mut() {
        session.pause().map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Resumes the active live session clock when policy allows.
pub fn resume_live_session() -> Result<(), String> {
    let mut guard = LIVE_SESSION
        .lock()
        .map_err(|_| "live session mutex poisoned".to_owned())?;
    if let Some(session) = guard.as_mut() {
        session.resume().map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Returns whether a live session is currently held.
#[must_use]
pub fn live_session_active() -> bool {
    LIVE_SESSION.lock().is_ok_and(|guard| guard.is_some())
}
