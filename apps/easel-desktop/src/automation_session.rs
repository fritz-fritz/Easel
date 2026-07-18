// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared automation store for profiles, schedules, and rotation queues.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use directories::ProjectDirs;
use easel_scheduler::{AutomationPaths, AutomationStore};

/// Opens the process-wide automation store.
pub fn automation_store() -> Result<std::sync::MutexGuard<'static, AutomationStore>, String> {
    static STORE: OnceLock<Mutex<AutomationStore>> = OnceLock::new();
    let mutex = STORE.get_or_init(|| {
        let (config_dir, data_dir) = dirs();
        Mutex::new(
            AutomationStore::open(AutomationPaths::new(config_dir, data_dir))
                .expect("open automation store"),
        )
    });
    mutex
        .lock()
        .map_err(|_| "automation store lock poisoned".into())
}

fn dirs() -> (PathBuf, PathBuf) {
    ProjectDirs::from("net", "fritztech", "easel").map_or_else(
        || {
            (
                PathBuf::from(".").join("easel-config"),
                PathBuf::from(".").join("easel-data"),
            )
        },
        |dirs| {
            (
                dirs.config_dir().to_path_buf(),
                dirs.data_dir().to_path_buf(),
            )
        },
    )
}
