// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! SQLite rotation apply history for avoid-repeat and CLI status.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use easel_core::{AssetId, ProfileId, RotationQueueId, ScheduleId};
use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

/// One recorded rotation apply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RotationHistoryEntry {
    /// Queue that produced the selection, when known.
    pub queue_id: Option<RotationQueueId>,
    /// Profile that received the wallpaper.
    pub profile_id: ProfileId,
    /// Schedule that triggered the apply, when known.
    pub schedule_id: Option<ScheduleId>,
    /// Selected asset.
    pub asset_id: AssetId,
    /// Explainable selection reason.
    pub reason: String,
    /// Unix epoch seconds when the apply completed.
    pub occurred_at: i64,
}

/// Persistent rotation history database.
pub struct RotationHistoryStore {
    conn: Connection,
    path: PathBuf,
}

impl RotationHistoryStore {
    /// Opens or creates a history database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RotationHistoryStoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS rotation_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                queue_id TEXT,
                profile_id TEXT NOT NULL,
                schedule_id TEXT,
                asset_id TEXT NOT NULL,
                reason TEXT NOT NULL,
                occurred_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_rotation_history_queue
                ON rotation_history(queue_id, occurred_at DESC);
            CREATE TABLE IF NOT EXISTS schedule_fire (
                schedule_id TEXT PRIMARY KEY NOT NULL,
                last_fired_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS dynamic_still_state (
                profile_id TEXT PRIMARY KEY NOT NULL,
                asset_id TEXT NOT NULL,
                key_label TEXT NOT NULL,
                applied_at INTEGER NOT NULL
            );
            ",
        )?;
        Ok(Self { conn, path })
    }

    /// Returns the database path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends a successful apply.
    pub fn record(&self, entry: &RotationHistoryEntry) -> Result<(), RotationHistoryStoreError> {
        self.conn.execute(
            "INSERT INTO rotation_history
                (queue_id, profile_id, schedule_id, asset_id, reason, occurred_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry.queue_id.map(RotationQueueId::to_hyphenated_string),
                entry.profile_id.to_hyphenated_string(),
                entry.schedule_id.map(ScheduleId::to_hyphenated_string),
                entry.asset_id.to_hyphenated_string(),
                entry.reason,
                entry.occurred_at,
            ],
        )?;
        Ok(())
    }

    /// Returns newest-first asset ids for avoid-repeat on a queue.
    pub fn recent_assets(
        &self,
        queue_id: RotationQueueId,
        limit: u32,
    ) -> Result<Vec<AssetId>, RotationHistoryStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT asset_id FROM rotation_history
             WHERE queue_id = ?1
             ORDER BY occurred_at DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![queue_id.to_hyphenated_string(), limit], |row| {
            let id: String = row.get(0)?;
            Ok(id)
        })?;
        let mut out = Vec::new();
        for row in rows {
            let id = row?;
            out.push(AssetId::parse(&id).map_err(|error| {
                RotationHistoryStoreError::Corrupt(format!("bad asset id: {error}"))
            })?);
        }
        Ok(out)
    }

    /// Returns the most recent history row, if any.
    pub fn latest(&self) -> Result<Option<RotationHistoryEntry>, RotationHistoryStoreError> {
        self.conn
            .query_row(
                "SELECT queue_id, profile_id, schedule_id, asset_id, reason, occurred_at
                 FROM rotation_history
                 ORDER BY occurred_at DESC, id DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()?
            .map(|(queue, profile, schedule, asset, reason, occurred_at)| {
                Ok(RotationHistoryEntry {
                    queue_id: queue
                        .map(|value| RotationQueueId::parse(&value))
                        .transpose()
                        .map_err(|error| {
                            RotationHistoryStoreError::Corrupt(format!("bad queue id: {error}"))
                        })?,
                    profile_id: ProfileId::parse(&profile).map_err(|error| {
                        RotationHistoryStoreError::Corrupt(format!("bad profile id: {error}"))
                    })?,
                    schedule_id: schedule
                        .map(|value| ScheduleId::parse(&value))
                        .transpose()
                        .map_err(|error| {
                            RotationHistoryStoreError::Corrupt(format!("bad schedule id: {error}"))
                        })?,
                    asset_id: AssetId::parse(&asset).map_err(|error| {
                        RotationHistoryStoreError::Corrupt(format!("bad asset id: {error}"))
                    })?,
                    reason,
                    occurred_at,
                })
            })
            .transpose()
    }

    /// Records the last fire time for a schedule.
    pub fn set_last_fired(
        &self,
        schedule_id: ScheduleId,
        occurred_at: i64,
    ) -> Result<(), RotationHistoryStoreError> {
        self.conn.execute(
            "INSERT INTO schedule_fire (schedule_id, last_fired_at) VALUES (?1, ?2)
             ON CONFLICT(schedule_id) DO UPDATE SET last_fired_at = excluded.last_fired_at",
            params![schedule_id.to_hyphenated_string(), occurred_at],
        )?;
        Ok(())
    }

    /// Returns the last fire time for a schedule.
    pub fn last_fired(
        &self,
        schedule_id: ScheduleId,
    ) -> Result<Option<i64>, RotationHistoryStoreError> {
        self.conn
            .query_row(
                "SELECT last_fired_at FROM schedule_fire WHERE schedule_id = ?1",
                params![schedule_id.to_hyphenated_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Returns the last applied dynamic-still frame for a profile, if any.
    pub fn dynamic_still_state(
        &self,
        profile_id: ProfileId,
    ) -> Result<Option<easel_core::AppliedDynamicFrame>, RotationHistoryStoreError> {
        self.conn
            .query_row(
                "SELECT asset_id, key_label, applied_at FROM dynamic_still_state
                 WHERE profile_id = ?1",
                params![profile_id.to_hyphenated_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?
            .map(|(asset, key_label, applied_at)| {
                Ok(easel_core::AppliedDynamicFrame {
                    asset_id: AssetId::parse(&asset).map_err(|error| {
                        RotationHistoryStoreError::Corrupt(format!("bad asset id: {error}"))
                    })?,
                    key_label,
                    applied_at,
                })
            })
            .transpose()
    }

    /// Records the last applied dynamic-still frame for catch-up decisions.
    pub fn set_dynamic_still_state(
        &self,
        profile_id: ProfileId,
        state: &easel_core::AppliedDynamicFrame,
    ) -> Result<(), RotationHistoryStoreError> {
        self.conn.execute(
            "INSERT INTO dynamic_still_state (profile_id, asset_id, key_label, applied_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(profile_id) DO UPDATE SET
                asset_id = excluded.asset_id,
                key_label = excluded.key_label,
                applied_at = excluded.applied_at",
            params![
                profile_id.to_hyphenated_string(),
                state.asset_id.to_hyphenated_string(),
                state.key_label,
                state.applied_at,
            ],
        )?;
        Ok(())
    }
}

/// Unix epoch seconds.
#[must_use]
pub fn now_unix_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

/// History store failures.
#[derive(Debug, Error)]
pub enum RotationHistoryStoreError {
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// SQLite failure.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    /// Corrupt row contents.
    #[error("corrupt rotation history: {0}")]
    Corrupt(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::AssetId;

    #[test]
    fn records_and_reads_recent() {
        let dir = std::env::temp_dir().join(format!("easel-rot-hist-{}", uuid::Uuid::new_v4()));
        let store = RotationHistoryStore::open(dir.join("history.db")).unwrap();
        let queue = RotationQueueId::new();
        let asset = AssetId::new();
        store
            .record(&RotationHistoryEntry {
                queue_id: Some(queue),
                profile_id: ProfileId::new(),
                schedule_id: None,
                asset_id: asset,
                reason: "test".into(),
                occurred_at: 100,
            })
            .unwrap();
        let recent = store.recent_assets(queue, 5).unwrap();
        assert_eq!(recent, vec![asset]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
