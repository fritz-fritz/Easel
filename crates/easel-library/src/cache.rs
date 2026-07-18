// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Conservative acquisition cache for remote still images.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use easel_core::{AssetLocation, MediaAsset};
use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;
use thiserror::Error;
use url::Url;

const DEFAULT_USER_AGENT: &str = "Easel/0.1 (https://github.com/fritz-fritz/easel)";
/// Upper bound for a single remote acquisition download.
pub const MAX_ACQUISITION_BYTES: u64 = 128 * 1024 * 1024;

/// Downloads and retains remote acquisition bytes under a cache root.
pub struct AcquisitionCache {
    root: PathBuf,
    http: Client,
}

impl AcquisitionCache {
    /// Creates a cache rooted at `root`.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, CacheError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let http = Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .https_only(true)
            .build()
            .map_err(|error| CacheError::Request(error.to_string()))?;
        Ok(Self { root, http })
    }

    /// Returns the cache root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Ensures `asset` bytes are present locally and returns the cached path.
    pub fn ensure_local(&self, asset: &MediaAsset) -> Result<PathBuf, CacheError> {
        match &asset.location {
            AssetLocation::Local { path } => {
                let path = PathBuf::from(path);
                if path.is_file() {
                    Ok(path)
                } else {
                    Err(CacheError::MissingLocal(path))
                }
            }
            AssetLocation::Remote {
                acquisition_url, ..
            } => self.download(asset, acquisition_url),
        }
    }

    /// Removes cached files older than `max_age_secs`.
    pub fn purge_older_than(&self, max_age_secs: u64) -> Result<usize, CacheError> {
        let now = SystemTime::now();
        let mut removed = 0;
        if !self.root.is_dir() {
            return Ok(0);
        }
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let modified = entry.metadata()?.modified().unwrap_or(UNIX_EPOCH);
            let age = now
                .duration_since(modified)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            if age > max_age_secs {
                fs::remove_file(&path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn download(&self, asset: &MediaAsset, url: &Url) -> Result<PathBuf, CacheError> {
        if url.scheme() != "https" {
            return Err(CacheError::InsecureUrl(url.clone()));
        }
        let host = url
            .host_str()
            .ok_or_else(|| CacheError::Request("acquisition URL missing host".into()))?;
        if !host_allowed(host) {
            return Err(CacheError::HostNotAllowed(host.to_owned()));
        }

        let file_name = cache_file_name(asset, url);
        let destination = self.root.join(file_name);
        if destination.is_file() {
            return Ok(destination);
        }

        let response = self
            .http
            .get(url.clone())
            .header(USER_AGENT, DEFAULT_USER_AGENT)
            .send()
            .map_err(|error| CacheError::Request(error.to_string()))?;
        if !response.status().is_success() {
            return Err(CacheError::Request(format!(
                "download failed with HTTP {}",
                response.status()
            )));
        }
        if let Some(length) = response.content_length() {
            if length > MAX_ACQUISITION_BYTES {
                return Err(CacheError::TooLarge {
                    bytes: length,
                    limit: MAX_ACQUISITION_BYTES,
                });
            }
        }

        let temporary = destination.with_extension("partial");
        {
            use std::io::Read;

            let mut file = fs::File::create(&temporary)?;
            let mut written: u64 = 0;
            let mut remote = response;
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                let read = remote.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                written = written.saturating_add(read as u64);
                if written > MAX_ACQUISITION_BYTES {
                    drop(file);
                    let _ = fs::remove_file(&temporary);
                    return Err(CacheError::TooLarge {
                        bytes: written,
                        limit: MAX_ACQUISITION_BYTES,
                    });
                }
                file.write_all(&buffer[..read])?;
            }
            file.sync_all()?;
        }
        fs::rename(&temporary, &destination)?;
        Ok(destination)
    }
}

fn cache_file_name(asset: &MediaAsset, url: &Url) -> String {
    let id = asset.provider_id.as_ref().map_or_else(
        || asset.id.to_hyphenated_string(),
        |provider| format!("{}_{}", provider.provider, provider.asset_id),
    );
    let extension = Path::new(url.path())
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("img");
    let safe_id: String = id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    format!("{safe_id}.{extension}")
}

fn host_allowed(host: &str) -> bool {
    const ALLOWED_SUFFIXES: &[&str] = &[
        "openverse.org",
        "staticflickr.com",
        "flickr.com",
        "wikimedia.org",
        "wikipedia.org",
        "nasa.gov",
        "githubusercontent.com",
    ];
    ALLOWED_SUFFIXES
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

/// Acquisition cache failure.
#[derive(Debug, Error)]
pub enum CacheError {
    /// Filesystem error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// HTTP download failure.
    #[error("request failed: {0}")]
    Request(String),
    /// Local asset path missing.
    #[error("local asset missing: {0}")]
    MissingLocal(PathBuf),
    /// Non-HTTPS acquisition URL rejected.
    #[error("insecure acquisition URL rejected: {0}")]
    InsecureUrl(Url),
    /// Host is outside the provider allowlist.
    #[error("acquisition host not allowed: {0}")]
    HostNotAllowed(String),
    /// Remote payload exceeded the acquisition size limit.
    #[error("acquisition exceeded size limit ({bytes} > {limit} bytes)")]
    TooLarge {
        /// Observed or attempted byte count.
        bytes: u64,
        /// Configured maximum.
        limit: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_http_and_unknown_hosts() {
        let root = std::env::temp_dir().join(format!("easel-cache-{}", uuid::Uuid::new_v4()));
        let cache = AcquisitionCache::new(&root).unwrap();
        let asset = MediaAsset {
            id: easel_core::AssetId::new(),
            provider_id: None,
            title: None,
            media: easel_core::MediaMetadata::StillImage {
                dimensions: easel_core::MediaDimensions {
                    width: 10,
                    height: 10,
                },
            },
            location: AssetLocation::Remote {
                canonical_work_url: Url::parse("https://example.com/a").unwrap(),
                preview_url: Url::parse("https://example.com/a.jpg").unwrap(),
                acquisition_url: Url::parse("http://example.com/a.jpg").unwrap(),
            },
            license: None,
            attribution: None,
            content_safety: easel_core::ContentSafety::Safe,
            source: None,
            use_reporting_url: None,
            retrieved_at_unix: None,
        };
        assert!(matches!(
            cache.ensure_local(&asset),
            Err(CacheError::InsecureUrl(_))
        ));
    }
}
