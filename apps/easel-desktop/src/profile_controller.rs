// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Profiles-page presentation model for saved compositions and display groups.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use easel_core::{DisplayGroup, DisplayId, Profile, ProfileId};
use serde_json::json;

use crate::automation_session::automation_store;
use crate::display_session::current_displays;

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
        #[qproperty(QStringList, profile_model)]
        #[qproperty(QStringList, group_model)]
        type ProfileController = super::ProfileControllerRust;

        #[qinvokable]
        #[rust_name = "refresh"]
        fn refresh(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "save_group_from_current"]
        fn saveGroupFromCurrent(self: Pin<&mut Self>, name: QString);

        #[qinvokable]
        #[rust_name = "delete_profile_at"]
        fn deleteProfileAt(self: Pin<&mut Self>, index: i32);
    }
}

/// Presentation state for the Profiles page.
pub struct ProfileControllerRust {
    status_text: QString,
    profile_model: QStringList,
    group_model: QStringList,
    profile_ids: Vec<ProfileId>,
}

impl Default for ProfileControllerRust {
    fn default() -> Self {
        let mut controller = Self {
            status_text: QString::from("Save compositions from Compose to reuse them."),
            profile_model: QStringList::default(),
            group_model: QStringList::default(),
            profile_ids: Vec::new(),
        };
        let _ = controller.reload_models();
        controller
    }
}

impl ProfileControllerRust {
    fn reload_models(&mut self) -> Result<(), String> {
        let store = automation_store()?;
        self.profile_ids = store.profiles().iter().map(|profile| profile.id).collect();
        self.profile_model = qstring_list(store.profiles().iter().map(|profile| {
            json!({
                "id": profile.id.to_hyphenated_string(),
                "name": profile.name,
                "displays": profile.displays.len(),
                "hasQueue": profile.rotation_queue_id.is_some(),
                "hasSchedule": profile.schedule_id.is_some(),
                "presentation": format!("{:?}", profile.presentation),
                "hasStillSet": profile.still_set_id.is_some(),
            })
            .to_string()
        }));
        self.group_model = qstring_list(store.groups().iter().map(|group| {
            json!({
                "id": group.id.to_hyphenated_string(),
                "name": group.name,
                "displays": group.displays.len(),
            })
            .to_string()
        }));
        self.status_text = QString::from(
            format!(
                "{} profile(s), {} display group(s)",
                store.profiles().len(),
                store.groups().len()
            )
            .as_str(),
        );
        Ok(())
    }
}

impl qobject::ProfileController {
    fn refresh(mut self: Pin<&mut Self>) {
        match self.as_mut().rust_mut().reload_models() {
            Ok(()) => {
                let status = self.as_ref().rust().status_text.clone();
                let profiles = self.as_ref().rust().profile_model.clone();
                let groups = self.as_ref().rust().group_model.clone();
                self.as_mut().set_status_text(status);
                self.as_mut().set_profile_model(profiles);
                self.as_mut().set_group_model(groups);
            }
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }

    fn save_group_from_current(mut self: Pin<&mut Self>, name: QString) {
        let name = name.to_string();
        let displays: Vec<DisplayId> = current_displays()
            .iter()
            .map(|display| display.id)
            .collect();
        let group = DisplayGroup::new(name, displays);
        match automation_store()
            .and_then(|mut store| store.upsert_group(group).map_err(|error| error.to_string()))
        {
            Ok(()) => {
                self.refresh();
            }
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }

    fn delete_profile_at(mut self: Pin<&mut Self>, index: i32) {
        let Some(id) = self
            .as_ref()
            .rust()
            .profile_ids
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            self.as_mut()
                .set_status_text(QString::from("Invalid profile index"));
            return;
        };
        match automation_store()
            .and_then(|mut store| store.delete_profile(id).map_err(|error| error.to_string()))
        {
            Ok(()) => self.refresh(),
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }
}

fn qstring_list(items: impl IntoIterator<Item = String>) -> QStringList {
    let mut list = QStringList::default();
    for item in items {
        list.append(QString::from(item.as_str()));
    }
    list
}

/// Saves a Compose snapshot as a named profile with optional schedule.
#[allow(clippy::too_many_arguments)]
pub fn save_compose_profile(
    name: &str,
    source_path: &str,
    fit_mode: easel_core::FitMode,
    layout_mode: easel_core::LayoutMode,
    zoom: f64,
    focal_x: f64,
    focal_y: f64,
    schedule_index: i32,
    media_mode_index: i32,
) -> Result<Profile, String> {
    use easel_core::{
        AssetId, AssetLocation, ContentSafety, DynamicStillSet, MediaAsset, MediaDimensions,
        MediaMetadata, PresentationMode, RotationQueue,
    };
    use easel_scheduler::AutomationStore;

    use crate::library_session::library_store;

    let displays = current_displays();
    let mut profile = Profile::new(name);
    profile.displays = displays.iter().map(|display| display.id).collect();
    profile.fit_mode = fit_mode;
    profile.layout_mode = layout_mode;
    profile.zoom = zoom;
    profile.focal_x = focal_x;
    profile.focal_y = focal_y;
    profile.presentation = match media_mode_index {
        1 => PresentationMode::DynamicStills,
        2 => PresentationMode::LiveMedia,
        _ => PresentationMode::Static,
    };
    if profile.presentation == PresentationMode::LiveMedia {
        return Err(
            "live media profiles require Stage 6; choose Still image or Dynamic stills".into(),
        );
    }

    let asset_id = AssetId::new();
    let asset = MediaAsset {
        id: asset_id,
        provider_id: None,
        title: Some(name.to_owned()),
        media: MediaMetadata::StillImage {
            dimensions: MediaDimensions {
                width: 1,
                height: 1,
            },
        },
        location: AssetLocation::Local {
            path: source_path.to_owned(),
        },
        license: None,
        attribution: None,
        content_safety: ContentSafety::Safe,
        source: None,
        use_reporting_url: None,
        retrieved_at_unix: None,
    };
    library_store()?
        .upsert_asset(&asset)
        .map_err(|error| error.to_string())?;
    profile.selected_asset = Some(asset_id);

    let queue = RotationQueue::from_assets(format!("{name} queue"), vec![asset_id]);
    profile.rotation_queue_id = Some(queue.id);

    let (still_set, schedule) = if profile.presentation == PresentationMode::DynamicStills {
        let set = DynamicStillSet::default_hourly(format!("{name} stills"), profile.id, asset_id)
            .map_err(|error| error.to_string())?;
        profile.still_set_id = Some(set.id);
        // Dynamic stills are not driven by compose schedule rotation.
        profile.schedule_id = None;
        (Some(set), None)
    } else {
        let schedule = AutomationStore::schedule_from_compose_index(
            profile.id,
            format!("{name} schedule"),
            schedule_index,
        )
        .map_err(|error| error.to_string())?;
        profile.schedule_id = Some(schedule.id);
        (None, Some(schedule))
    };

    let mut store = automation_store()?;
    store
        .upsert_queue(queue)
        .map_err(|error| error.to_string())?;
    if let Some(schedule) = schedule {
        store
            .upsert_schedule(schedule)
            .map_err(|error| error.to_string())?;
    }
    if let Some(still_set) = still_set {
        store
            .upsert_still_set(still_set)
            .map_err(|error| error.to_string())?;
    }
    store
        .upsert_profile(profile.clone())
        .map_err(|error| error.to_string())?;
    Ok(profile)
}

/// Imports an Apple/Plasma dynamic HEIC into the library and saves a dynamic profile.
pub fn import_dynamic_heic_profile(
    heic_path: &str,
    name: &str,
    fit_mode: easel_core::FitMode,
    layout_mode: easel_core::LayoutMode,
    zoom: f64,
    focal_x: f64,
    focal_y: f64,
) -> Result<(Profile, easel_core::DynamicStillSet, String), String> {
    use std::path::PathBuf;

    use easel_core::{PresentationMode, RotationQueue};
    use easel_dynamic::{import_dynamic_heic, persist_imported_desktop};

    use crate::library_session::library_store;

    let imported = import_dynamic_heic(heic_path).map_err(|error| error.to_string())?;
    let displays = current_displays();
    let mut profile = Profile::new(name);
    profile.displays = displays.iter().map(|display| display.id).collect();
    profile.fit_mode = fit_mode;
    profile.layout_mode = layout_mode;
    profile.zoom = zoom;
    profile.focal_x = focal_x;
    profile.focal_y = focal_y;
    profile.presentation = PresentationMode::DynamicStills;
    // Dynamic stills are driven by the still-set poller / native host — not schedule rotation.
    profile.schedule_id = None;

    let asset_dir =
        crate::library_session::dynamic_stills_dir().join(profile.id.to_hyphenated_string());

    let persisted = persist_imported_desktop(&imported, name, profile.id, &asset_dir)
        .map_err(|error| error.to_string())?;
    {
        let library = library_store()?;
        for asset in &persisted.assets {
            library
                .upsert_asset(asset)
                .map_err(|error| error.to_string())?;
        }
    }

    let first_path = persisted
        .frame_paths
        .first()
        .map(PathBuf::as_path)
        .map(|path| path.display().to_string())
        .ok_or_else(|| "imported HEIC produced no frames".to_string())?;

    profile.selected_asset = Some(persisted.still_set.fallback_asset_id);
    profile.still_set_id = Some(persisted.still_set.id);
    let queue = RotationQueue::from_assets(
        format!("{name} queue"),
        vec![persisted.still_set.fallback_asset_id],
    );
    profile.rotation_queue_id = Some(queue.id);

    let mut store = automation_store()?;
    store
        .upsert_queue(queue)
        .map_err(|error| error.to_string())?;
    store
        .upsert_still_set(persisted.still_set.clone())
        .map_err(|error| error.to_string())?;
    store
        .upsert_profile(profile.clone())
        .map_err(|error| error.to_string())?;

    Ok((profile, persisted.still_set, first_path))
}
