// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic wallpaper rotation and schedule evaluation.

#![forbid(unsafe_code)]

mod catalog;
mod select;
mod solar;
mod tick;

pub use catalog::{AutomationCatalog, CatalogError};
pub use select::{SelectionError, SelectionOutcome, select_next_asset};
pub use solar::{SolarEvent, solar_events_for_day};
pub use tick::{ScheduleEvaluation, TickDecision, evaluate_schedule, tick};
