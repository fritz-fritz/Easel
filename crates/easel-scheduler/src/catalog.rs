// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Versioned on-disk automation catalog (profiles, groups, queues, schedules).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use easel_core::{
    AutomationState, DisplayGroup, DisplayGroupError, MissingOutputPolicy, Profile,
    ProfileValidationError, RotationQueue, RotationQueueError, Schedule, ScheduleValidationError,
};

/// Current catalog schema version.
pub const CATALOG_SCHEMA_VERSION: u16 = 1;

/// Human-readable TOML catalog for Stage 4 automation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomationCatalog {
    /// Serialized schema version.
    pub schema_version: u16,
    /// Saved wallpaper profiles.
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Reusable display groups.
    #[serde(default)]
    pub display_groups: Vec<DisplayGroup>,
    /// Independent rotation queues.
    #[serde(default)]
    pub rotation_queues: Vec<RotationQueue>,
    /// Automation schedules.
    #[serde(default)]
    pub schedules: Vec<Schedule>,
    /// Hotplug policy for missing outputs.
    #[serde(default)]
    pub missing_output_policy: MissingOutputPolicy,
    /// Runtime automation state.
    #[serde(default)]
    pub state: AutomationState,
}

impl Default for AutomationCatalog {
    fn default() -> Self {
        Self {
            schema_version: CATALOG_SCHEMA_VERSION,
            profiles: Vec::new(),
            display_groups: Vec::new(),
            rotation_queues: Vec::new(),
            schedules: Vec::new(),
            missing_output_policy: MissingOutputPolicy::default(),
            state: AutomationState::idle(),
        }
    }
}

impl AutomationCatalog {
    /// Validates schema and every nested record.
    pub fn validate(&self) -> Result<(), CatalogError> {
        if self.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(CatalogError::UnsupportedSchema(self.schema_version));
        }
        self.state.validate()?;
        for profile in &self.profiles {
            profile.validate()?;
        }
        for group in &self.display_groups {
            group.validate()?;
        }
        for queue in &self.rotation_queues {
            queue.validate()?;
        }
        for schedule in &self.schedules {
            schedule.validate()?;
        }
        Ok(())
    }

    /// Returns a profile by identity.
    #[must_use]
    pub fn profile(&self, id: easel_core::ProfileId) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    /// Returns a display group by identity.
    #[must_use]
    pub fn display_group(&self, id: easel_core::DisplayGroupId) -> Option<&DisplayGroup> {
        self.display_groups.iter().find(|group| group.id == id)
    }

    /// Returns a rotation queue by identity.
    #[must_use]
    pub fn rotation_queue(&self, id: easel_core::RotationQueueId) -> Option<&RotationQueue> {
        self.rotation_queues.iter().find(|queue| queue.id == id)
    }

    /// Returns a schedule by identity.
    #[must_use]
    pub fn schedule(&self, id: easel_core::ScheduleId) -> Option<&Schedule> {
        self.schedules.iter().find(|schedule| schedule.id == id)
    }

    /// Upserts a profile by identity.
    pub fn upsert_profile(&mut self, profile: Profile) {
        if let Some(existing) = self.profiles.iter_mut().find(|item| item.id == profile.id) {
            *existing = profile;
        } else {
            self.profiles.push(profile);
        }
    }

    /// Upserts a display group by identity.
    pub fn upsert_display_group(&mut self, group: DisplayGroup) {
        if let Some(existing) = self
            .display_groups
            .iter_mut()
            .find(|item| item.id == group.id)
        {
            *existing = group;
        } else {
            self.display_groups.push(group);
        }
    }

    /// Upserts a rotation queue by identity.
    pub fn upsert_rotation_queue(&mut self, queue: RotationQueue) {
        if let Some(existing) = self
            .rotation_queues
            .iter_mut()
            .find(|item| item.id == queue.id)
        {
            *existing = queue;
        } else {
            self.rotation_queues.push(queue);
        }
    }

    /// Upserts a schedule by identity.
    pub fn upsert_schedule(&mut self, schedule: Schedule) {
        if let Some(existing) = self
            .schedules
            .iter_mut()
            .find(|item| item.id == schedule.id)
        {
            *existing = schedule;
        } else {
            self.schedules.push(schedule);
        }
    }
}

/// Invalid automation catalog.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CatalogError {
    /// Schema version is not supported by this build.
    #[error("unsupported automation catalog schema version: {0}")]
    UnsupportedSchema(u16),
    /// Nested automation state failed validation.
    #[error(transparent)]
    State(#[from] easel_core::AutomationStateError),
    /// Nested profile failed validation.
    #[error(transparent)]
    Profile(#[from] ProfileValidationError),
    /// Nested display group failed validation.
    #[error(transparent)]
    DisplayGroup(#[from] DisplayGroupError),
    /// Nested rotation queue failed validation.
    #[error(transparent)]
    RotationQueue(#[from] RotationQueueError),
    /// Nested schedule failed validation.
    #[error(transparent)]
    Schedule(#[from] ScheduleValidationError),
}
