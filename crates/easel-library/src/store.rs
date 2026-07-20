// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! SQLite-backed library metadata for assets, collections, favorites, and history.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use easel_core::{AssetId, Collection, CollectionId, HistoryAction, HistoryEvent, MediaAsset};
use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;
/// Persistent library database.
pub struct LibraryStore {
    conn: Connection,
    path: PathBuf,
}

impl LibraryStore {
    /// Opens or creates a library database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LibraryStoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS assets (
                id TEXT PRIMARY KEY NOT NULL,
                json TEXT NOT NULL,
                path TEXT,
                provider TEXT,
                provider_asset_id TEXT,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS folders (
                path TEXT PRIMARY KEY NOT NULL,
                recursive INTEGER NOT NULL DEFAULT 1,
                added_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS favorites (
                asset_id TEXT PRIMARY KEY NOT NULL,
                favorited_at INTEGER NOT NULL,
                FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY NOT NULL,
                asset_id TEXT NOT NULL,
                action TEXT NOT NULL,
                occurred_at INTEGER NOT NULL,
                FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_assets_path ON assets(path);
            CREATE INDEX IF NOT EXISTS idx_history_occurred ON history(occurred_at DESC);
            ",
        )?;
        Ok(Self { conn, path })
    }

    /// Returns the database path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Upserts a media asset and optional local path index key.
    pub fn upsert_asset(&self, asset: &MediaAsset) -> Result<(), LibraryStoreError> {
        let id = asset_id_string(asset.id);
        let json = serde_json::to_string(asset)?;
        let path = match &asset.location {
            easel_core::AssetLocation::Local { path } => Some(path.as_str()),
            easel_core::AssetLocation::Remote { .. } => None,
        };
        let (provider, provider_asset_id) = match &asset.provider_id {
            Some(provider_id) => (
                Some(provider_id.provider.as_str()),
                Some(provider_id.asset_id.as_str()),
            ),
            None => (None, None),
        };
        self.conn.execute(
            "INSERT INTO assets (id, json, path, provider, provider_asset_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                json = excluded.json,
                path = excluded.path,
                provider = excluded.provider,
                provider_asset_id = excluded.provider_asset_id,
                updated_at = excluded.updated_at",
            params![id, json, path, provider, provider_asset_id, now_unix_i64()],
        )?;
        Ok(())
    }

    /// Loads an asset by local library identity.
    pub fn get_asset(&self, id: AssetId) -> Result<Option<MediaAsset>, LibraryStoreError> {
        let id = asset_id_string(id);
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT json FROM assets WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(LibraryStoreError::from)
    }

    /// Finds a previously indexed local file by absolute path.
    pub fn find_by_path(&self, path: &str) -> Result<Option<MediaAsset>, LibraryStoreError> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT json FROM assets WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(LibraryStoreError::from)
    }

    /// Deletes an indexed local asset by absolute path.
    pub fn remove_by_path(&self, path: &str) -> Result<bool, LibraryStoreError> {
        let changed = self
            .conn
            .execute("DELETE FROM assets WHERE path = ?1", params![path])?;
        Ok(changed > 0)
    }

    /// Lists indexed assets ordered by most recently updated.
    pub fn list_assets(&self, limit: usize) -> Result<Vec<MediaAsset>, LibraryStoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT json FROM assets ORDER BY updated_at DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit_i64(limit)], |row| row.get::<_, String>(0))?;
        let mut assets = Vec::new();
        for row in rows {
            assets.push(serde_json::from_str(&row?)?);
        }
        Ok(assets)
    }

    /// Registers a watched folder root.
    pub fn add_folder(&self, path: &str, recursive: bool) -> Result<(), LibraryStoreError> {
        self.conn.execute(
            "INSERT INTO folders (path, recursive, added_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET recursive = excluded.recursive",
            params![path, i64::from(recursive), now_unix_i64()],
        )?;
        Ok(())
    }

    /// Removes a watched folder root.
    pub fn remove_folder(&self, path: &str) -> Result<(), LibraryStoreError> {
        self.conn
            .execute("DELETE FROM folders WHERE path = ?1", params![path])?;
        Ok(())
    }

    /// Lists registered folder roots.
    pub fn list_folders(&self) -> Result<Vec<(String, bool)>, LibraryStoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, recursive FROM folders ORDER BY path")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0))
        })?;
        let mut folders = Vec::new();
        for row in rows {
            folders.push(row?);
        }
        Ok(folders)
    }

    /// Marks an asset as a favorite.
    pub fn add_favorite(&self, asset_id: AssetId) -> Result<(), LibraryStoreError> {
        self.conn.execute(
            "INSERT INTO favorites (asset_id, favorited_at) VALUES (?1, ?2)
             ON CONFLICT(asset_id) DO UPDATE SET favorited_at = excluded.favorited_at",
            params![asset_id_string(asset_id), now_unix_i64()],
        )?;
        self.record_history(&HistoryEvent::new(
            asset_id,
            HistoryAction::Favorited,
            now_unix(),
        ))?;
        Ok(())
    }

    /// Removes a favorite marker.
    pub fn remove_favorite(&self, asset_id: AssetId) -> Result<(), LibraryStoreError> {
        self.conn.execute(
            "DELETE FROM favorites WHERE asset_id = ?1",
            params![asset_id_string(asset_id)],
        )?;
        Ok(())
    }

    /// Returns whether an asset is favorited.
    pub fn is_favorite(&self, asset_id: AssetId) -> Result<bool, LibraryStoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM favorites WHERE asset_id = ?1",
            params![asset_id_string(asset_id)],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Lists favorite assets newest-first.
    pub fn list_favorites(&self, limit: usize) -> Result<Vec<MediaAsset>, LibraryStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT a.json FROM favorites f
             JOIN assets a ON a.id = f.asset_id
             ORDER BY f.favorited_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit_i64(limit)], |row| row.get::<_, String>(0))?;
        let mut assets = Vec::new();
        for row in rows {
            assets.push(serde_json::from_str(&row?)?);
        }
        Ok(assets)
    }

    /// Creates or replaces a collection record.
    pub fn upsert_collection(&self, collection: &Collection) -> Result<(), LibraryStoreError> {
        collection
            .validate()
            .map_err(|error| LibraryStoreError::InvalidCollection(error.to_string()))?;
        let json = serde_json::to_string(collection)?;
        self.conn.execute(
            "INSERT INTO collections (id, name, json) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, json = excluded.json",
            params![collection.id.to_hyphenated_string(), collection.name, json],
        )?;
        Ok(())
    }

    /// Loads a collection by id.
    pub fn get_collection(
        &self,
        id: CollectionId,
    ) -> Result<Option<Collection>, LibraryStoreError> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT json FROM collections WHERE id = ?1",
                params![id.to_hyphenated_string()],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(LibraryStoreError::from)
    }

    /// Lists all collections.
    pub fn list_collections(&self) -> Result<Vec<Collection>, LibraryStoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT json FROM collections ORDER BY name")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut collections = Vec::new();
        for row in rows {
            collections.push(serde_json::from_str(&row?)?);
        }
        Ok(collections)
    }

    /// Appends a history event.
    pub fn record_history(&self, event: &HistoryEvent) -> Result<(), LibraryStoreError> {
        self.conn.execute(
            "INSERT INTO history (id, asset_id, action, occurred_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                event.id.to_hyphenated_string(),
                asset_id_string(event.asset_id),
                history_action_key(event.action),
                u64_to_i64(event.occurred_at_unix)
            ],
        )?;
        Ok(())
    }

    /// Lists recent history events newest-first.
    pub fn list_history(&self, limit: usize) -> Result<Vec<HistoryEvent>, LibraryStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, asset_id, action, occurred_at FROM history
             ORDER BY occurred_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit_i64(limit)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (id, asset_id, action, occurred_at) = row?;
            events.push(HistoryEvent {
                id: easel_core::HistoryEventId::parse(&id).map_err(|error| {
                    LibraryStoreError::Corrupt(format!("bad history id: {error}"))
                })?,
                asset_id: parse_asset_id(&asset_id)?,
                action: parse_history_action(&action)?,
                occurred_at_unix: u64::try_from(occurred_at).unwrap_or(0),
            });
        }
        Ok(events)
    }
}

/// Library persistence failure.
#[derive(Debug, Error)]
pub enum LibraryStoreError {
    /// SQLite error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Filesystem error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialize/deserialize failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// Collection failed validation.
    #[error("invalid collection: {0}")]
    InvalidCollection(String),
    /// Stored rows were corrupt.
    #[error("corrupt library data: {0}")]
    Corrupt(String),
}

fn asset_id_string(id: AssetId) -> String {
    id.to_hyphenated_string()
}

fn parse_asset_id(value: &str) -> Result<AssetId, LibraryStoreError> {
    AssetId::parse(value)
        .map_err(|error| LibraryStoreError::Corrupt(format!("bad asset id: {error}")))
}

fn history_action_key(action: HistoryAction) -> &'static str {
    match action {
        HistoryAction::Discovered => "discovered",
        HistoryAction::Previewed => "previewed",
        HistoryAction::Applied => "applied",
        HistoryAction::Favorited => "favorited",
        HistoryAction::Collected => "collected",
    }
}

fn parse_history_action(value: &str) -> Result<HistoryAction, LibraryStoreError> {
    match value {
        "discovered" => Ok(HistoryAction::Discovered),
        "previewed" => Ok(HistoryAction::Previewed),
        "applied" => Ok(HistoryAction::Applied),
        "favorited" => Ok(HistoryAction::Favorited),
        "collected" => Ok(HistoryAction::Collected),
        other => Err(LibraryStoreError::Corrupt(format!(
            "unknown history action: {other}"
        ))),
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn now_unix_i64() -> i64 {
    u64_to_i64(now_unix())
}

fn limit_i64(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX)
}

fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{AssetLocation, Attribution, MediaDimensions, MediaMetadata, ProviderAssetId};
    use url::Url;
    use uuid::Uuid;

    fn sample_asset(path: &str) -> MediaAsset {
        MediaAsset {
            id: AssetId::new(),
            provider_id: None,
            title: Some("Local".into()),
            media: MediaMetadata::StillImage {
                dimensions: MediaDimensions {
                    width: 100,
                    height: 80,
                },
            },
            location: AssetLocation::Local { path: path.into() },
            license: None,
            attribution: None,
            content_safety: easel_core::ContentSafety::Safe,
            source: None,
            use_reporting_url: None,
            retrieved_at_unix: None,
        }
    }

    #[test]
    fn persists_assets_favorites_and_history() {
        let dir = std::env::temp_dir().join(format!("easel-lib-{}", Uuid::new_v4()));
        let db = dir.join("library.db");
        let store = LibraryStore::open(&db).expect("open");
        let asset = sample_asset("/tmp/photo.png");
        store.upsert_asset(&asset).expect("upsert");
        store.add_favorite(asset.id).expect("favorite");
        assert!(store.is_favorite(asset.id).expect("is favorite"));
        let favorites = store.list_favorites(10).expect("list favorites");
        assert_eq!(favorites.len(), 1);
        let history = store.list_history(10).expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].action, HistoryAction::Favorited);
    }

    #[test]
    fn persists_remote_provenance() {
        let dir = std::env::temp_dir().join(format!("easel-lib-{}", Uuid::new_v4()));
        let store = LibraryStore::open(dir.join("library.db")).expect("open");
        let asset = MediaAsset {
            id: AssetId::new(),
            provider_id: Some(ProviderAssetId {
                provider: "openverse".into(),
                asset_id: "abc".into(),
            }),
            title: Some("Mountains".into()),
            media: MediaMetadata::StillImage {
                dimensions: MediaDimensions {
                    width: 1024,
                    height: 680,
                },
            },
            location: AssetLocation::Remote {
                canonical_work_url: Url::parse("https://example.com/work").unwrap(),
                preview_url: Url::parse("https://example.com/preview.jpg").unwrap(),
                acquisition_url: Url::parse("https://example.com/full.jpg").unwrap(),
            },
            license: None,
            attribution: Some(Attribution {
                creator_name: "Ada".into(),
                creator_url: None,
                text: "Ada".into(),
            }),
            content_safety: easel_core::ContentSafety::Safe,
            source: Some("flickr".into()),
            use_reporting_url: None,
            retrieved_at_unix: Some(1),
        };
        store.upsert_asset(&asset).expect("upsert");
        let loaded = store.get_asset(asset.id).expect("get").expect("present");
        assert_eq!(loaded.attribution.unwrap().creator_name, "Ada");
        assert_eq!(loaded.source.as_deref(), Some("flickr"));
    }
}
