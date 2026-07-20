// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Filesystem watching for indexed library folders.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;

use crate::probe::local_media_extension;

/// Normalized folder watch notifications.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FolderWatchEvent {
    /// A media path was created or modified.
    Upsert(PathBuf),
    /// A media path was removed.
    Remove(PathBuf),
}

/// Watches registered folders for indexed media changes.
pub struct FolderWatcher {
    _watcher: RecommendedWatcher,
    receiver: Receiver<FolderWatchEvent>,
}

impl FolderWatcher {
    /// Starts watching `folders` recursively.
    pub fn start(folders: &[impl AsRef<Path>]) -> Result<Self, WatchError> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |result: Result<notify::Event, notify::Error>| {
                let Ok(event) = result else {
                    return;
                };
                for path in event.paths {
                    let extension = path
                        .extension()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default();
                    if !local_media_extension(extension) {
                        continue;
                    }
                    let message = match event.kind {
                        EventKind::Remove(_) => FolderWatchEvent::Remove(path),
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any => {
                            FolderWatchEvent::Upsert(path)
                        }
                        EventKind::Access(_) | EventKind::Other => continue,
                    };
                    let _ = tx.send(message);
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        for folder in folders {
            let path = folder.as_ref();
            if path.is_dir() {
                watcher.watch(path, RecursiveMode::Recursive)?;
            }
        }

        Ok(Self {
            _watcher: watcher,
            receiver: rx,
        })
    }

    /// Drains pending events without blocking.
    pub fn drain(&self) -> Vec<FolderWatchEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.receiver.try_recv() {
            events.push(event);
        }
        events
    }

    /// Blocks until the next event arrives.
    pub fn recv(&self) -> Result<FolderWatchEvent, WatchError> {
        self.receiver.recv().map_err(|_| WatchError::ChannelClosed)
    }
}

/// Folder watcher failure.
#[derive(Debug, Error)]
pub enum WatchError {
    /// Underlying notify error.
    #[error("watch error: {0}")]
    Notify(#[from] notify::Error),
    /// Event channel closed.
    #[error("watch channel closed")]
    ChannelClosed,
}
