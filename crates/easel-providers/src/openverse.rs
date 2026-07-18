// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Openverse still-image search adapter.

use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use easel_core::{
    AssetId, AssetLicense, AssetLocation, Attribution, ContentSafety, MediaAsset, MediaDimensions,
    MediaMetadata, ProviderAssetId,
};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use url::Url;

use crate::{ImageProvider, ProviderError, ProviderMetadata, SearchPage, SearchQuery, policy};

const DEFAULT_BASE_URL: &str = "https://api.openverse.org/v1/";
const DEFAULT_USER_AGENT: &str = "Easel/0.1 (https://github.com/fritz-fritz/easel)";

/// Runtime configuration for the Openverse HTTP client.
#[derive(Clone, Debug)]
pub struct OpenverseConfig {
    /// API base URL ending with `/v1/`.
    pub base_url: Url,
    /// Optional OAuth access token.
    pub access_token: Option<String>,
    /// HTTP user agent identifying Easel.
    pub user_agent: String,
}

impl Default for OpenverseConfig {
    fn default() -> Self {
        Self {
            base_url: Url::parse(DEFAULT_BASE_URL).expect("static openverse base URL"),
            access_token: None,
            user_agent: DEFAULT_USER_AGENT.into(),
        }
    }
}

impl OpenverseConfig {
    /// Builds configuration from process environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(token) = std::env::var("EASEL_OPENVERSE_ACCESS_TOKEN") {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                config.access_token = Some(trimmed.to_owned());
            }
        }
        config
    }
}

/// Openverse image search client.
pub struct OpenverseClient {
    http: Client,
    config: OpenverseConfig,
}

impl OpenverseClient {
    /// Creates a client with the given configuration.
    pub fn new(config: OpenverseConfig) -> Result<Self, ProviderError> {
        let http = Client::builder()
            .user_agent(config.user_agent.clone())
            .https_only(true)
            .build()
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        Ok(Self { http, config })
    }

    /// Parses a previously fetched JSON body into a normalized page.
    pub fn parse_search_response(body: &str) -> Result<SearchPage, ProviderError> {
        let response: OpenverseSearchResponse = serde_json::from_str(body)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        normalize_search_response(response)
    }
}

#[async_trait]
impl ImageProvider for OpenverseClient {
    fn metadata(&self) -> ProviderMetadata {
        policy::OPENVERSE
    }

    async fn search(&self, query: &SearchQuery) -> Result<SearchPage, ProviderError> {
        let mut url = self
            .config
            .base_url
            .join("images/")
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            if !query.text.trim().is_empty() {
                pairs.append_pair("q", query.text.trim());
            }
            if let Some(width) = query.minimum_width {
                pairs.append_pair("size", size_bucket(width));
            }
            if let Some(license_type) = query.license.openverse_license_type() {
                // Public-domain uses explicit license CSV instead.
                if query.license.openverse_licenses().is_none() {
                    pairs.append_pair("license_type", license_type);
                }
            }
            if let Some(licenses) = query.license.openverse_licenses() {
                pairs.append_pair("license", licenses);
            }
            if !query.sources.is_empty() {
                pairs.append_pair("source", &query.sources.join(","));
            }
            let page = query
                .cursor
                .as_deref()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(1);
            pairs.append_pair("page", &page.to_string());
            let page_size = query.page_size.unwrap_or(20).clamp(1, 50);
            pairs.append_pair("page_size", &page_size.to_string());
            pairs.append_pair("filter_dead", "true");
        }

        let mut request = self
            .http
            .get(url)
            .header(USER_AGENT, self.config.user_agent.as_str());
        if let Some(token) = &self.config.access_token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }

        let response = request
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        let status = response.status();
        if status.as_u16() == 429 {
            return Err(ProviderError::RateLimited);
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Request(format!(
                "Openverse returned HTTP {status}: {body}"
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        Self::parse_search_response(&body)
    }
}

fn size_bucket(minimum_width: u32) -> &'static str {
    if minimum_width >= 2000 {
        "large"
    } else if minimum_width >= 500 {
        "medium"
    } else {
        "small"
    }
}

fn normalize_search_response(
    response: OpenverseSearchResponse,
) -> Result<SearchPage, ProviderError> {
    let retrieved_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    let mut assets = Vec::with_capacity(response.results.len());
    for item in response.results {
        assets.push(normalize_image(item, retrieved_at_unix)?);
    }

    let next_cursor = if response.page < response.page_count && response.page_count > 0 {
        Some((response.page + 1).to_string())
    } else {
        None
    };

    Ok(SearchPage {
        assets,
        next_cursor,
        result_count: Some(response.result_count),
    })
}

fn normalize_image(
    item: OpenverseImage,
    retrieved_at_unix: u64,
) -> Result<MediaAsset, ProviderError> {
    let canonical_work_url = parse_required_url(&item.foreign_landing_url, "foreign_landing_url")?;
    let preview_url = parse_required_url(
        item.thumbnail.as_deref().unwrap_or(item.url.as_str()),
        "thumbnail/url",
    )?;
    let acquisition_url = parse_required_url(&item.url, "url")?;
    let license_url = parse_required_url(&item.license_url, "license_url")?;

    let width = item.width.unwrap_or(0);
    let height = item.height.unwrap_or(0);
    if width == 0 || height == 0 {
        return Err(ProviderError::InvalidResponse(format!(
            "image {} missing dimensions",
            item.id
        )));
    }

    let creator_name = item
        .creator
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Unknown creator".into());
    let creator_url = item
        .creator_url
        .as_deref()
        .and_then(|value| Url::parse(value).ok());
    let attribution_text = item.attribution.clone().unwrap_or_else(|| {
        format!(
            "\"{}\" by {creator_name} is licensed under {}.",
            item.title.as_deref().unwrap_or("Untitled"),
            item.license
        )
    });

    Ok(MediaAsset {
        id: AssetId::new(),
        provider_id: Some(ProviderAssetId {
            provider: policy::OPENVERSE.id.into(),
            asset_id: item.id,
        }),
        title: item.title,
        media: MediaMetadata::StillImage {
            dimensions: MediaDimensions { width, height },
        },
        location: AssetLocation::Remote {
            canonical_work_url,
            preview_url,
            acquisition_url,
        },
        license: Some(AssetLicense {
            identifier: item.license,
            version: item.license_version,
            url: license_url,
        }),
        attribution: Some(Attribution {
            creator_name,
            creator_url,
            text: attribution_text,
        }),
        content_safety: if item.mature.unwrap_or(false) {
            ContentSafety::Mature
        } else {
            ContentSafety::Safe
        },
        source: item.source.or(item.provider),
        use_reporting_url: None,
        retrieved_at_unix: Some(retrieved_at_unix),
    })
}

fn parse_required_url(value: &str, field: &str) -> Result<Url, ProviderError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ProviderError::InvalidResponse(format!(
            "missing required URL field {field}"
        )));
    }
    Url::parse(trimmed)
        .map_err(|error| ProviderError::InvalidResponse(format!("invalid URL in {field}: {error}")))
}

#[derive(Debug, Deserialize)]
struct OpenverseSearchResponse {
    result_count: u32,
    page_count: u32,
    page: u32,
    results: Vec<OpenverseImage>,
}

#[derive(Debug, Deserialize)]
struct OpenverseImage {
    id: String,
    title: Option<String>,
    foreign_landing_url: String,
    url: String,
    creator: Option<String>,
    creator_url: Option<String>,
    license: String,
    license_version: Option<String>,
    license_url: String,
    provider: Option<String>,
    source: Option<String>,
    attribution: Option<String>,
    mature: Option<bool>,
    height: Option<u32>,
    width: Option<u32>,
    thumbnail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_search_page() {
        let body = include_str!("../tests/fixtures/openverse_images_search.json");
        let page = OpenverseClient::parse_search_response(body).expect("fixture parses");
        assert_eq!(page.assets.len(), 2);
        assert_eq!(page.next_cursor.as_deref(), Some("2"));
        assert_eq!(page.result_count, Some(240));

        let first = &page.assets[0];
        assert_eq!(
            first.provider_id.as_ref().map(|id| id.asset_id.as_str()),
            Some("f561777d-24ea-483f-9cf8-f17aa1fd6aa3")
        );
        assert_eq!(first.media.dimensions().width, 1024);
        assert_eq!(first.media.dimensions().height, 680);
        assert!(first.license.is_some());
        assert!(first.attribution.is_some());
        assert_eq!(first.source.as_deref(), Some("flickr"));
        match &first.location {
            AssetLocation::Remote {
                canonical_work_url, ..
            } => {
                assert!(canonical_work_url.as_str().contains("flickr.com"));
            }
            AssetLocation::Local { .. } => panic!("expected remote asset"),
        }
    }
}
