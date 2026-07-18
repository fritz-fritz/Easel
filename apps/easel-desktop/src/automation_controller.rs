// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Automation-page presentation model and tray/CLI-aligned controls.

use std::path::PathBuf;
use std::pin::Pin;

use cxx_qt_lib::{QString, QStringList};
use easel_core::{AssetId, MissingOutputPolicy};
use serde_json::json;

use crate::apply_service;
use crate::automation_session;
use crate::library_session;

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
        #[qproperty(bool, paused)]
        #[qproperty(QString, status_text)]
        #[qproperty(QString, last_decision)]
        #[qproperty(QStringList, schedule_model)]
        #[qproperty(QStringList, queue_model)]
        #[qproperty(i32, interval_seconds)]
        #[qproperty(i32, policy_index)]
        type AutomationController = super::AutomationControllerRust;

        #[qinvokable]
        #[rust_name = "refresh"]
        fn refresh(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "set_automation_paused"]
        fn setAutomationPaused(self: Pin<&mut Self>, paused: bool);

        #[qinvokable]
        #[rust_name = "skip_next"]
        fn skipNext(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "run_tick"]
        fn runTick(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "save_interval"]
        fn saveInterval(self: Pin<&mut Self>, seconds: i32);

        #[qinvokable]
        #[rust_name = "apply_policy_index"]
        fn applyPolicyIndex(self: Pin<&mut Self>, index: i32);

        #[qinvokable]
        #[rust_name = "build_queue_from_favorites"]
        fn buildQueueFromFavorites(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "enqueue_path"]
        fn enqueuePath(self: Pin<&mut Self>, path: QString);
    }
}

/// Presentation state for the Automation page.
pub struct AutomationControllerRust {
    paused: bool,
    status_text: QString,
    last_decision: QString,
    schedule_model: QStringList,
    queue_model: QStringList,
    interval_seconds: i32,
    policy_index: i32,
}

impl Default for AutomationControllerRust {
    fn default() -> Self {
        Self {
            paused: false,
            status_text: QString::from("Configure schedules and rotation queues"),
            last_decision: QString::default(),
            schedule_model: QStringList::default(),
            queue_model: QStringList::default(),
            interval_seconds: 3600,
            policy_index: 0,
        }
    }
}

impl qobject::AutomationController {
    fn refresh(mut self: Pin<&mut Self>) {
        match publish() {
            Ok(snapshot) => {
                self.as_mut().set_paused(snapshot.paused);
                self.as_mut().set_last_decision(snapshot.last_decision);
                self.as_mut().set_schedule_model(snapshot.schedules);
                self.as_mut().set_queue_model(snapshot.queues);
                self.as_mut()
                    .set_interval_seconds(snapshot.interval_seconds);
                self.as_mut().set_policy_index(snapshot.policy_index);
                self.as_mut().set_status_text(snapshot.status);
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Refresh failed: {error}").as_str()));
            }
        }
    }

    fn set_automation_paused(mut self: Pin<&mut Self>, paused: bool) {
        match automation_session::set_paused(paused) {
            Ok(()) => {
                self.as_mut().set_paused(paused);
                self.as_mut().set_status_text(QString::from(if paused {
                    "Automation paused"
                } else {
                    "Automation resumed"
                }));
                self.refresh();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Pause failed: {error}").as_str()));
            }
        }
    }

    fn skip_next(mut self: Pin<&mut Self>) {
        match apply_service::run_automation_tick(true) {
            Ok(message) => {
                self.as_mut()
                    .set_status_text(QString::from(message.as_str()));
                self.refresh();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Skip failed: {error}").as_str()));
            }
        }
    }

    fn run_tick(mut self: Pin<&mut Self>) {
        match apply_service::run_automation_tick(false) {
            Ok(message) => {
                self.as_mut()
                    .set_status_text(QString::from(message.as_str()));
                self.refresh();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Tick failed: {error}").as_str()));
            }
        }
    }

    fn save_interval(mut self: Pin<&mut Self>, seconds: i32) {
        let seconds = u64::try_from(seconds.max(1)).unwrap_or(1);
        match automation_session::set_interval_schedule("Interval", seconds) {
            Ok(schedule) => {
                self.as_mut()
                    .set_interval_seconds(i32::try_from(seconds).unwrap_or(i32::MAX));
                self.as_mut().set_status_text(QString::from(
                    format!(
                        "Interval schedule {} every {seconds}s",
                        schedule.id.to_hyphenated_string()
                    )
                    .as_str(),
                ));
                self.refresh();
            }
            Err(error) => {
                self.as_mut().set_status_text(QString::from(
                    format!("Interval save failed: {error}").as_str(),
                ));
            }
        }
    }

    fn apply_policy_index(mut self: Pin<&mut Self>, index: i32) {
        let policy = match index {
            1 => MissingOutputPolicy::PauseUntilRestored,
            2 => MissingOutputPolicy::RequireAny,
            _ => MissingOutputPolicy::SkipMissing,
        };
        match automation_session::set_missing_output_policy(policy) {
            Ok(()) => {
                self.as_mut().set_policy_index(index);
                self.as_mut().set_status_text(QString::from(
                    format!("Hotplug policy set to {policy:?}").as_str(),
                ));
                self.refresh();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Policy failed: {error}").as_str()));
            }
        }
    }

    fn build_queue_from_favorites(mut self: Pin<&mut Self>) {
        let result = (|| {
            let store = library_session::library_store()?;
            let favorites = store
                .list_favorites(64)
                .map_err(|error| error.to_string())?;
            let assets: Vec<AssetId> = favorites.into_iter().map(|asset| asset.id).collect();
            if assets.is_empty() {
                return Err("favorite at least one library asset first".into());
            }
            automation_session::set_rotation_queue("Favorites", assets)
        })();
        match result {
            Ok(queue) => {
                self.as_mut().set_status_text(QString::from(
                    format!(
                        "Rotation queue '{}' with {} asset(s)",
                        queue.name,
                        queue.assets.len()
                    )
                    .as_str(),
                ));
                self.refresh();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Queue failed: {error}").as_str()));
            }
        }
    }

    fn enqueue_path(mut self: Pin<&mut Self>, path: QString) {
        let path = PathBuf::from(path.to_string());
        let result = (|| {
            let asset = automation_session::find_asset_by_path(&path).ok_or_else(|| {
                "path is not in the library index; add the folder under Library first".to_owned()
            })?;
            let mut assets = Vec::new();
            {
                let catalog = automation_session::lock()?;
                if let Some(queue_id) = catalog.state.active_queue_id {
                    if let Some(queue) = catalog.rotation_queue(queue_id) {
                        assets.clone_from(&queue.assets);
                    }
                }
            }
            if !assets.contains(&asset.id) {
                assets.push(asset.id);
            }
            automation_session::set_rotation_queue("Manual queue", assets)
        })();
        match result {
            Ok(queue) => {
                self.as_mut().set_status_text(QString::from(
                    format!("Queue now has {} asset(s)", queue.assets.len()).as_str(),
                ));
                self.refresh();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Enqueue failed: {error}").as_str()));
            }
        }
    }
}

struct Snapshot {
    paused: bool,
    status: QString,
    last_decision: QString,
    schedules: QStringList,
    queues: QStringList,
    interval_seconds: i32,
    policy_index: i32,
}

fn publish() -> Result<Snapshot, String> {
    let catalog = automation_session::lock()?;
    let mut schedules = QStringList::default();
    let mut interval_seconds = 3600_i32;
    for schedule in &catalog.schedules {
        if let easel_core::ScheduleKind::Interval { seconds } = schedule.kind {
            if catalog.state.active_schedule_id == Some(schedule.id) {
                interval_seconds = i32::try_from(seconds).unwrap_or(3600);
            }
        }
        let payload = json!({
            "id": schedule.id.to_hyphenated_string(),
            "name": schedule.name,
            "enabled": schedule.enabled,
            "kind": format!("{:?}", schedule.kind),
            "active": catalog.state.active_schedule_id == Some(schedule.id),
        })
        .to_string();
        schedules.append_clone(&QString::from(payload.as_str()));
    }
    let mut queues = QStringList::default();
    for queue in &catalog.rotation_queues {
        let payload = json!({
            "id": queue.id.to_hyphenated_string(),
            "name": queue.name,
            "assets": queue.assets.len(),
            "avoidRepeat": queue.avoid_repeat,
            "active": catalog.state.active_queue_id == Some(queue.id),
        })
        .to_string();
        queues.append_clone(&QString::from(payload.as_str()));
    }
    let policy_index = match catalog.missing_output_policy {
        MissingOutputPolicy::SkipMissing => 0,
        MissingOutputPolicy::PauseUntilRestored => 1,
        MissingOutputPolicy::RequireAny => 2,
    };
    let status_line = if catalog.state.paused {
        "paused=true"
    } else {
        "paused=false"
    };
    Ok(Snapshot {
        paused: catalog.state.paused,
        status: QString::from(status_line),
        last_decision: QString::from(catalog.state.last_decision.as_str()),
        schedules,
        queues,
        interval_seconds,
        policy_index,
    })
}
