// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Profiles-page presentation model.

#![allow(clippy::too_many_arguments)]

use std::path::PathBuf;
use std::pin::Pin;

use cxx_qt_lib::{QString, QStringList};
use serde_json::json;

use crate::apply_service;
use crate::automation_session;
use crate::display_session;

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
        #[qproperty(QStringList, profile_model)]
        #[qproperty(QStringList, group_model)]
        #[qproperty(QString, status_text)]
        #[qproperty(QString, selected_profile_id)]
        type ProfileController = super::ProfileControllerRust;

        #[qinvokable]
        #[rust_name = "refresh"]
        fn refresh(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "save_from_compose"]
        fn saveFromCompose(
            self: Pin<&mut Self>,
            name: QString,
            source_path: QString,
            fit_mode_index: i32,
            layout_mode_index: i32,
            zoom: f64,
            focal_x: f64,
            focal_y: f64,
        );

        #[qinvokable]
        #[rust_name = "activate_profile"]
        fn activateProfile(self: Pin<&mut Self>, index: i32);

        #[qinvokable]
        #[rust_name = "ensure_default_group"]
        fn ensureDefaultGroup(self: Pin<&mut Self>);
    }
}

/// Presentation state for the Profiles page.
pub struct ProfileControllerRust {
    profile_model: QStringList,
    group_model: QStringList,
    status_text: QString,
    selected_profile_id: QString,
}

impl Default for ProfileControllerRust {
    fn default() -> Self {
        Self {
            profile_model: QStringList::default(),
            group_model: QStringList::default(),
            status_text: QString::from("Save Compose settings as a reusable profile"),
            selected_profile_id: QString::default(),
        }
    }
}

impl qobject::ProfileController {
    fn refresh(mut self: Pin<&mut Self>) {
        match publish_models() {
            Ok((profiles, groups, selected)) => {
                self.as_mut().set_profile_model(profiles);
                self.as_mut().set_group_model(groups);
                self.as_mut().set_selected_profile_id(selected);
                self.as_mut()
                    .set_status_text(QString::from("Profiles refreshed"));
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Refresh failed: {error}").as_str()));
            }
        }
    }

    fn save_from_compose(
        mut self: Pin<&mut Self>,
        name: QString,
        source_path: QString,
        fit_mode_index: i32,
        layout_mode_index: i32,
        zoom: f64,
        focal_x: f64,
        focal_y: f64,
    ) {
        let name = name.to_string();
        let name = if name.trim().is_empty() {
            "Compose profile".to_owned()
        } else {
            name
        };
        let source = {
            let raw = source_path.to_string();
            if raw.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(raw))
            }
        };
        let profile = apply_service::profile_from_compose(
            &name,
            source.as_deref(),
            fit_mode_index,
            layout_mode_index,
            zoom,
            focal_x,
            focal_y,
        );
        match automation_session::save_profile(profile.clone()) {
            Ok(()) => {
                self.as_mut().set_selected_profile_id(QString::from(
                    profile.id.to_hyphenated_string().as_str(),
                ));
                self.as_mut().set_status_text(QString::from(
                    format!("Saved profile '{}'", profile.name).as_str(),
                ));
                self.refresh();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Save failed: {error}").as_str()));
            }
        }
    }

    fn activate_profile(mut self: Pin<&mut Self>, index: i32) {
        let Ok(mut catalog) = automation_session::lock() else {
            self.as_mut()
                .set_status_text(QString::from("Catalog unavailable"));
            return;
        };
        let Ok(index) = usize::try_from(index) else {
            self.as_mut()
                .set_status_text(QString::from("Unknown profile index"));
            return;
        };
        let Some(profile) = catalog.profiles.get(index).cloned() else {
            self.as_mut()
                .set_status_text(QString::from("Unknown profile index"));
            return;
        };
        catalog.state.active_profile_id = Some(profile.id);
        if let Some(schedule_id) = profile.schedule_id {
            catalog.state.active_schedule_id = Some(schedule_id);
        }
        if let Some(queue_id) = profile.rotation_queue_id {
            catalog.state.active_queue_id = Some(queue_id);
        }
        let name = profile.name.clone();
        let id = profile.id.to_hyphenated_string();
        if let Err(error) = automation_session::save(&catalog) {
            self.as_mut()
                .set_status_text(QString::from(format!("Activate failed: {error}").as_str()));
            return;
        }
        drop(catalog);
        self.as_mut()
            .set_selected_profile_id(QString::from(id.as_str()));
        self.as_mut().set_status_text(QString::from(
            format!("Activated profile '{name}'").as_str(),
        ));
        self.refresh();
    }

    fn ensure_default_group(mut self: Pin<&mut Self>) {
        let live = display_session::current_displays();
        match automation_session::ensure_default_group(&live) {
            Ok(group) => {
                self.as_mut().set_status_text(QString::from(
                    format!(
                        "Display group '{}' has {} member(s)",
                        group.name,
                        group.displays.len()
                    )
                    .as_str(),
                ));
                self.refresh();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Group failed: {error}").as_str()));
            }
        }
    }
}

fn publish_models() -> Result<(QStringList, QStringList, QString), String> {
    let catalog = automation_session::lock()?;
    let mut profiles = QStringList::default();
    for profile in &catalog.profiles {
        let payload = json!({
            "id": profile.id.to_hyphenated_string(),
            "name": profile.name,
            "displays": profile.displays.len(),
            "fit": format!("{:?}", profile.fit_mode).to_ascii_lowercase(),
            "layout": format!("{:?}", profile.layout_mode).to_ascii_lowercase(),
            "active": catalog.state.active_profile_id == Some(profile.id),
        })
        .to_string();
        profiles.append_clone(&QString::from(payload.as_str()));
    }
    let mut groups = QStringList::default();
    for group in &catalog.display_groups {
        let payload = json!({
            "id": group.id.to_hyphenated_string(),
            "name": group.name,
            "displays": group.displays.len(),
        })
        .to_string();
        groups.append_clone(&QString::from(payload.as_str()));
    }
    let selected = catalog
        .state
        .active_profile_id
        .map(easel_core::ProfileId::to_hyphenated_string)
        .unwrap_or_default();
    Ok((profiles, groups, QString::from(selected.as_str())))
}
