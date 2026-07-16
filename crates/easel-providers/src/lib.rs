// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Policy-aware online still-image provider contracts.

#![forbid(unsafe_code)]

use async_trait::async_trait;
use easel_core::MediaAsset;
use thiserror::Error;

/// Whether current published terms allow an adapter to be enabled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyDisposition {
    /// Current terms allow the planned integration.
    Allowed,
    /// The provider must give written approval for this product.
    RequiresWrittenApproval,
    /// Current terms prohibit the planned integration.
    Prohibited,
    /// Terms have not been reviewed or are ambiguous.
    Unknown,
}

/// Static provider identity and compliance evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderMetadata {
    /// Stable internal key.
    pub id: &'static str,
    /// User-visible provider name.
    pub display_name: &'static str,
    /// Current policy disposition.
    pub disposition: PolicyDisposition,
    /// Official terms or API-guideline URL reviewed for the disposition.
    pub terms_url: &'static str,
}

/// Search filters shared by compatible providers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchQuery {
    /// User-entered search text.
    pub text: String,
    /// Minimum image width.
    pub minimum_width: Option<u32>,
    /// Minimum image height.
    pub minimum_height: Option<u32>,
    /// Provider cursor from the previous page.
    pub cursor: Option<String>,
}

/// One normalized provider page.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchPage {
    /// Still-image assets with provenance intact.
    pub assets: Vec<MediaAsset>,
    /// Opaque cursor for the next page.
    pub next_cursor: Option<String>,
}

/// Online catalog adapter.
#[async_trait]
pub trait ImageProvider: Send + Sync {
    /// Returns static compliance and display metadata.
    fn metadata(&self) -> ProviderMetadata;

    /// Searches the catalog and normalizes results.
    async fn search(&self, query: &SearchQuery) -> Result<SearchPage, ProviderError>;

    /// Records an image-use action when required by approved provider terms.
    async fn record_use(&self, _asset: &MediaAsset) -> Result<(), ProviderError> {
        Ok(())
    }
}

/// Registry that refuses to activate disallowed providers.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: Vec<Box<dyn ImageProvider>>,
}

impl ProviderRegistry {
    /// Adds a provider only when its reviewed disposition is allowed.
    pub fn register(&mut self, provider: Box<dyn ImageProvider>) -> Result<(), ProviderError> {
        let metadata = provider.metadata();
        if metadata.disposition != PolicyDisposition::Allowed {
            return Err(ProviderError::PolicyBlocked {
                provider: metadata.id,
                disposition: metadata.disposition,
            });
        }
        self.providers.push(provider);
        Ok(())
    }

    /// Returns the number of enabled providers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Returns whether no providers are enabled.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

/// Provider discovery failure.
#[derive(Debug, Error)]
pub enum ProviderError {
    /// The adapter is disabled by its reviewed policy.
    #[error("provider {provider} is blocked by policy: {disposition:?}")]
    PolicyBlocked {
        /// Stable provider key.
        provider: &'static str,
        /// Reviewed disposition.
        disposition: PolicyDisposition,
    },
    /// Network operation failed.
    #[error("provider request failed: {0}")]
    Request(String),
    /// Provider response was not valid for the normalized contract.
    #[error("provider response was invalid: {0}")]
    InvalidResponse(String),
}

/// Reviewed metadata for planned built-in providers.
pub mod policy {
    use super::{PolicyDisposition, ProviderMetadata};

    /// Openverse is the initial compliant discovery candidate.
    pub const OPENVERSE: ProviderMetadata = ProviderMetadata {
        id: "openverse",
        display_name: "Openverse",
        disposition: PolicyDisposition::Allowed,
        terms_url: "https://docs.openverse.org/terms_of_service.html",
    };

    /// Unsplash remains disabled without written use-case approval.
    pub const UNSPLASH: ProviderMetadata = ProviderMetadata {
        id: "unsplash",
        display_name: "Unsplash",
        disposition: PolicyDisposition::RequiresWrittenApproval,
        terms_url: "https://help.unsplash.com/en/articles/2511257-guideline-replicating-unsplash",
    };

    /// Pexels' published API guidance excludes wallpaper applications.
    pub const PEXELS: ProviderMetadata = ProviderMetadata {
        id: "pexels",
        display_name: "Pexels",
        disposition: PolicyDisposition::Prohibited,
        terms_url: "https://www.pexels.com/api/documentation/",
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BlockedProvider;

    #[async_trait]
    impl ImageProvider for BlockedProvider {
        fn metadata(&self) -> ProviderMetadata {
            policy::PEXELS
        }

        async fn search(&self, _query: &SearchQuery) -> Result<SearchPage, ProviderError> {
            Ok(SearchPage::default())
        }
    }

    #[test]
    fn blocked_provider_cannot_be_registered() {
        let mut registry = ProviderRegistry::default();
        let result = registry.register(Box::new(BlockedProvider));
        assert!(matches!(result, Err(ProviderError::PolicyBlocked { .. })));
        assert!(registry.is_empty());
    }
}
