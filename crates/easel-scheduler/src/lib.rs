// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Profile, schedule, and rotation persistence plus automation runtime helpers.

#![forbid(unsafe_code)]

mod history;
mod store;

pub use history::{
    RotationHistoryEntry, RotationHistoryStore, RotationHistoryStoreError, now_unix_i64,
};
pub use store::{
    AutomationPaths, AutomationStore, AutomationStoreError, AutomationSummary, DueDynamicStill,
};
