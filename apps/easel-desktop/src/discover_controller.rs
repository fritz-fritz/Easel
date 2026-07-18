// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Discover-page Openverse search and acquisition presentation model.

#![allow(clippy::large_enum_variant)]

use std::pin::Pin;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use easel_core::{HistoryAction, HistoryEvent, MediaAsset, PixelBudget, assess_suitability};
use easel_providers::{
    ImageProvider, LicenseFilter, OpenverseClient, OpenverseConfig, SearchQuery,
};
use serde_json::json;
use url::Url;

use crate::display_session::current_displays;
use crate::library_session::{acquisition_cache, library_store};

#[cxx_qt::bridge]
mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status_text)]
        #[qproperty(bool, busy)]
        #[qproperty(QString, query_text)]
        #[qproperty(i32, license_filter_index)]
        #[qproperty(QStringList, result_model)]
        #[qproperty(i32, result_count)]
        #[qproperty(bool, has_more)]
        #[qproperty(QString, acquired_file_url)]
        type DiscoverController = super::DiscoverControllerRust;

        #[qinvokable]
        #[rust_name = "search"]
        fn search(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "load_more"]
        fn loadMore(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "use_result"]
        fn useResult(self: Pin<&mut Self>, index: i32);

        #[qinvokable]
        #[rust_name = "favorite_result"]
        fn favoriteResult(self: Pin<&mut Self>, index: i32);
    }

    impl cxx_qt::Threading for DiscoverController {}
}

/// Presentation state for the Discover page.
pub struct DiscoverControllerRust {
    status_text: QString,
    busy: bool,
    query_text: QString,
    license_filter_index: i32,
    result_model: QStringList,
    result_count: i32,
    has_more: bool,
    acquired_file_url: QString,
    assets: Vec<MediaAsset>,
    next_cursor: Option<String>,
    job_generation: AtomicU64,
    job_tx: Sender<DiscoverJob>,
}

impl Default for DiscoverControllerRust {
    fn default() -> Self {
        Self {
            status_text: QString::from(
                "Search Openverse for openly licensed still images. License metadata is retained and should be verified on the source page.",
            ),
            busy: false,
            query_text: QString::default(),
            license_filter_index: 2,
            result_model: QStringList::default(),
            result_count: 0,
            has_more: false,
            acquired_file_url: QString::default(),
            assets: Vec::new(),
            next_cursor: None,
            job_generation: AtomicU64::new(0),
            job_tx: worker_sender(),
        }
    }
}

impl qobject::DiscoverController {
    fn search(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().assets.clear();
        self.as_mut().rust_mut().next_cursor = None;
        self.as_mut().set_result_model(QStringList::default());
        self.as_mut().set_result_count(0);
        self.as_mut().set_has_more(false);
        self.start_search(None);
    }

    fn load_more(self: Pin<&mut Self>) {
        let cursor = self.as_ref().rust().next_cursor.clone();
        if cursor.is_none() || *self.busy() {
            return;
        }
        self.start_search(cursor);
    }

    fn use_result(mut self: Pin<&mut Self>, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(asset) = self.as_ref().rust().assets.get(index).cloned() else {
            self.as_mut()
                .set_status_text(QString::from("Select a search result first"));
            return;
        };

        let generation = self
            .as_mut()
            .rust_mut()
            .job_generation
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        let qt_thread = self.qt_thread();
        let job_tx = self.as_ref().rust().job_tx.clone();
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status_text(QString::from("Downloading image for Compose…"));
        let _ = job_tx.send(DiscoverJob::Acquire {
            generation,
            asset,
            qt_thread,
        });
    }

    fn favorite_result(mut self: Pin<&mut Self>, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(asset) = self.as_ref().rust().assets.get(index).cloned() else {
            return;
        };
        match library_store() {
            Ok(store) => {
                if let Err(error) = store.upsert_asset(&asset) {
                    self.as_mut().set_status_text(QString::from(
                        format!("Could not save asset: {error}").as_str(),
                    ));
                    return;
                }
                if let Err(error) = store.add_favorite(asset.id) {
                    self.as_mut().set_status_text(QString::from(
                        format!("Could not favorite asset: {error}").as_str(),
                    ));
                    return;
                }
                self.as_mut()
                    .set_status_text(QString::from("Saved to favorites with provenance"));
            }
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }

    fn start_search(mut self: Pin<&mut Self>, cursor: Option<String>) {
        let text = self.query_text().to_string();
        if text.trim().is_empty() {
            self.as_mut()
                .set_status_text(QString::from("Enter a search query"));
            return;
        }

        let license = license_from_index(*self.license_filter_index());
        let append = cursor.is_some();
        let generation = self
            .as_mut()
            .rust_mut()
            .job_generation
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        let qt_thread = self.qt_thread();
        let job_tx = self.as_ref().rust().job_tx.clone();
        let budget = PixelBudget::from_displays(&current_displays());

        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status_text(QString::from("Searching Openverse…"));

        let _ = job_tx.send(DiscoverJob::Search {
            generation,
            query: SearchQuery {
                text,
                minimum_width: Some(budget.max_display_width.max(1)),
                minimum_height: None,
                license,
                sources: Vec::new(),
                cursor,
                page_size: Some(18),
            },
            budget,
            append,
            qt_thread,
        });
    }
}

enum DiscoverJob {
    Search {
        generation: u64,
        query: SearchQuery,
        budget: PixelBudget,
        append: bool,
        qt_thread: CxxQtThread<qobject::DiscoverController>,
    },
    Acquire {
        generation: u64,
        asset: MediaAsset,
        qt_thread: CxxQtThread<qobject::DiscoverController>,
    },
}

fn worker_sender() -> Sender<DiscoverJob> {
    static SENDER: OnceLock<Sender<DiscoverJob>> = OnceLock::new();
    SENDER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            thread::Builder::new()
                .name("easel-discover-worker".into())
                .spawn(move || worker_loop(rx))
                .expect("discover worker thread");
            tx
        })
        .clone()
}

fn worker_loop(rx: Receiver<DiscoverJob>) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("discover tokio runtime");

    while let Ok(job) = rx.recv() {
        match job {
            DiscoverJob::Search {
                generation,
                query,
                budget,
                append,
                qt_thread,
            } => run_search_job(&runtime, generation, query, budget, append, qt_thread),
            DiscoverJob::Acquire {
                generation,
                asset,
                qt_thread,
            } => run_acquire_job(generation, asset, qt_thread),
        }
    }
}

fn run_search_job(
    runtime: &tokio::runtime::Runtime,
    generation: u64,
    query: SearchQuery,
    budget: PixelBudget,
    append: bool,
    qt_thread: CxxQtThread<qobject::DiscoverController>,
) {
    let result = runtime.block_on(async {
        let client =
            OpenverseClient::new(OpenverseConfig::from_env()).map_err(|error| error.to_string())?;
        client
            .search(&query)
            .await
            .map_err(|error| error.to_string())
    });
    let _ = qt_thread.queue(move |mut controller| {
        let current = controller
            .as_ref()
            .rust()
            .job_generation
            .load(Ordering::SeqCst);
        if current != generation {
            return;
        }
        controller.as_mut().set_busy(false);
        match result {
            Ok(page) => apply_search_page(&mut controller, page, budget, append),
            Err(error) => {
                controller
                    .as_mut()
                    .set_status_text(QString::from(format!("Search failed: {error}").as_str()));
            }
        }
    });
}

fn apply_search_page(
    controller: &mut Pin<&mut qobject::DiscoverController>,
    page: easel_providers::SearchPage,
    budget: PixelBudget,
    append: bool,
) {
    if !append {
        controller.as_mut().rust_mut().assets.clear();
    }
    controller
        .as_mut()
        .rust_mut()
        .assets
        .extend(page.assets.iter().cloned());
    controller
        .as_mut()
        .rust_mut()
        .next_cursor
        .clone_from(&page.next_cursor);
    let model = build_result_model(&controller.as_ref().rust().assets, budget);
    let count = i32::try_from(controller.as_ref().rust().assets.len()).unwrap_or(i32::MAX);
    controller.as_mut().set_result_model(model);
    controller.as_mut().set_result_count(count);
    controller.as_mut().set_has_more(page.next_cursor.is_some());
    let total = page
        .result_count
        .unwrap_or_else(|| u32::try_from(count).unwrap_or(0));
    controller.as_mut().set_status_text(QString::from(
        format!(
            "Showing {count} of about {total} Openverse results. Verify license on the source page before reuse."
        )
        .as_str(),
    ));
}

fn run_acquire_job(
    generation: u64,
    asset: MediaAsset,
    qt_thread: CxxQtThread<qobject::DiscoverController>,
) {
    let result = (|| {
        if let Ok(store) = library_store() {
            store
                .upsert_asset(&asset)
                .map_err(|error| error.to_string())?;
            store
                .record_history(&HistoryEvent::new(
                    asset.id,
                    HistoryAction::Previewed,
                    now_unix(),
                ))
                .map_err(|error| error.to_string())?;
        }
        let cache = acquisition_cache()?;
        let path = cache
            .ensure_local(&asset)
            .map_err(|error| error.to_string())?;
        Url::from_file_path(&path).map_err(|()| "could not build file URL".to_owned())
    })();
    let _ = qt_thread.queue(move |mut controller| {
        let current = controller
            .as_ref()
            .rust()
            .job_generation
            .load(Ordering::SeqCst);
        if current != generation {
            return;
        }
        controller.as_mut().set_busy(false);
        match result {
            Ok(url) => {
                controller
                    .as_mut()
                    .set_acquired_file_url(QString::from(url.as_str()));
                controller
                    .as_mut()
                    .set_status_text(QString::from("Image ready — opening in Compose"));
            }
            Err(error) => {
                controller
                    .as_mut()
                    .set_status_text(QString::from(format!("Download failed: {error}").as_str()));
            }
        }
    });
}

fn build_result_model(assets: &[MediaAsset], budget: PixelBudget) -> QStringList {
    let mut list = QStringList::default();
    for asset in assets {
        let assessment = assess_suitability(asset.media.dimensions(), budget);
        let preview = match &asset.location {
            easel_core::AssetLocation::Remote { preview_url, .. } => {
                preview_url.as_str().to_owned()
            }
            easel_core::AssetLocation::Local { path } => path.clone(),
        };
        let canonical = match &asset.location {
            easel_core::AssetLocation::Remote {
                canonical_work_url, ..
            } => canonical_work_url.as_str().to_owned(),
            easel_core::AssetLocation::Local { .. } => String::new(),
        };
        let creator = asset
            .attribution
            .as_ref()
            .map_or_else(|| "Unknown".into(), |value| value.creator_name.clone());
        let license = asset.license.as_ref().map_or_else(
            || "unknown".into(),
            |value| match &value.version {
                Some(version) => format!("{} {}", value.identifier, version),
                None => value.identifier.clone(),
            },
        );
        let attribution = asset
            .attribution
            .as_ref()
            .map(|value| value.text.clone())
            .unwrap_or_default();
        let payload = json!({
            "title": asset.title.clone().unwrap_or_else(|| "Untitled".into()),
            "creator": creator,
            "license": license,
            "attribution": attribution,
            "preview": preview,
            "canonical": canonical,
            "width": asset.media.dimensions().width,
            "height": asset.media.dimensions().height,
            "score": assessment.score,
            "meetsMinimum": assessment.meets_minimum,
            "source": asset.source.clone().unwrap_or_default(),
        });
        list.append_clone(&QString::from(payload.to_string().as_str()));
    }
    list
}

fn license_from_index(index: i32) -> LicenseFilter {
    match index {
        1 => LicenseFilter::PublicDomain,
        2 => LicenseFilter::Commercial,
        _ => LicenseFilter::All,
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
