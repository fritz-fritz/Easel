// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Platform-independent Easel domain model.

#![forbid(unsafe_code)]

mod arrangement;
mod asset;
mod collection;
mod display;
mod display_group;
mod dynamic_still;
mod history;
mod hotplug;
mod layout_fixtures;
mod physical;
mod profile;
mod rotation;
mod schedule;
mod suitability;

pub use arrangement::{
    ARRANGEMENT_SCHEMA_VERSION, ArrangementError, DisplayArrangement, DisplayEvidence,
    ObservedDisplay, approximate_physical_origin, match_displays,
};
pub use asset::{
    AssetId, AssetLicense, AssetLocation, Attribution, ContentSafety, FrameRate, MediaAsset,
    MediaDimensions, MediaMetadata, ProviderAssetId,
};
pub use collection::{Collection, CollectionError, CollectionId};
pub use display::{
    Display, DisplayId, DisplayValidationError, LogicalRect, Millimeters, NativePixelSize,
    PhysicalPoint, PhysicalSize, ScaleFactor,
};
pub use display_group::{DisplayGroup, DisplayGroupError, DisplayGroupId};
pub use dynamic_still::{
    AppearanceMode, AppliedDynamicFrame, DYNAMIC_STILL_SET_SCHEMA_VERSION, DynamicEvalContext,
    DynamicScheduleKind, DynamicStillError, DynamicStillFrame, DynamicStillKey, DynamicStillSet,
    DynamicStillSetId, FrameSelection, TransitionDecision, active_frame_at,
    active_frame_with_context, decide_transition, next_transition_after, solar_sample_distance,
};
pub use history::{HistoryAction, HistoryEvent, HistoryEventId};
pub use hotplug::{
    HOTPLUG_POLICY_SCHEMA_VERSION, HotplugError, HotplugPolicy, HotplugResolution,
    MissingOutputPolicy, resolve_displays,
};
pub use layout_fixtures::{
    all_layout_fixtures, asymmetric_bezels, different_physical_same_resolution,
    mixed_scale_factors, negative_logical_origin, one_display, portrait_plus_landscape,
    same_physical_different_resolution, t_shaped, two_equal_row, vertical_stack,
};
pub use physical::{
    BezelInsets, MM_PER_INCH, PhysicalLayoutError, PhysicalRect, PhysicalSizeSource, Ppi,
    content_bounds, content_rect, panel_rect, physical_size_for_ppi, snap_origin,
};
pub use profile::{
    FitMode, LayoutMode, LoopMode, PROFILE_SCHEMA_VERSION, PlaybackPolicy, PresentationMode,
    Profile, ProfileId, ProfileValidationError,
};
pub use rotation::{
    ROTATION_QUEUE_SCHEMA_VERSION, RotationError, RotationPolicy, RotationQueue, RotationQueueId,
    RotationSource, SelectionDecision, select_next, skip_current,
};
pub use schedule::{
    InstantSeconds, LocalCivilTime, LocalTimeOfDay, SCHEDULE_SCHEMA_VERSION, Schedule,
    ScheduleError, ScheduleId, ScheduleRule, SolarEvent, explain_fire, instant_at_local,
    next_fire_after, solar_event_local_minutes, solar_position_deg,
};
pub use suitability::{PixelBudget, SuitabilityAssessment, assess_suitability};
