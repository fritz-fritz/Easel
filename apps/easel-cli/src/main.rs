// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Command-line controls for Easel profiles and automation.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use easel_core::{
    AssetId, InstantSeconds, ProfileId, RotationSource, explain_fire, next_fire_after,
};
use easel_scheduler::{AutomationPaths, AutomationStore, now_unix_i64};

#[derive(Debug, Parser)]
#[command(name = "easel", about = "Easel wallpaper automation controls")]
struct Cli {
    /// Fixed local offset from UTC in minutes (for time-of-day / solar evaluation).
    #[arg(long, global = true, default_value_t = 0)]
    utc_offset_minutes: i32,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// List saved profiles.
    Profiles,
    /// List schedules and next fire hints.
    Schedules,
    /// List dynamic still sets.
    Stills,
    /// Show automation status (pause, next fire, last apply).
    Status,
    /// Pause all rotation queues.
    Pause,
    /// Resume all rotation queues.
    Resume,
    /// Skip the next candidate on a profile's rotation queue.
    Skip {
        /// Profile id (hyphenated UUID). When omitted, uses the first profile with a queue.
        #[arg(long)]
        profile: Option<String>,
    },
    /// Print the next selection decision for a profile without applying.
    Next {
        /// Profile id (hyphenated UUID).
        #[arg(long)]
        profile: Option<String>,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let mut store = open_store()?;
    match cli.command {
        Commands::Profiles => {
            list_profiles(&store);
            Ok(())
        }
        Commands::Schedules => list_schedules(&store, cli.utc_offset_minutes),
        Commands::Stills => {
            list_stills(&store);
            Ok(())
        }
        Commands::Status => show_status(&store, cli.utc_offset_minutes),
        Commands::Pause => {
            store
                .set_all_paused(true)
                .map_err(|error| error.to_string())?;
            println!("All rotation queues paused.");
            Ok(())
        }
        Commands::Resume => {
            store
                .set_all_paused(false)
                .map_err(|error| error.to_string())?;
            println!("All rotation queues resumed.");
            Ok(())
        }
        Commands::Skip { profile } => skip_profile(&mut store, profile.as_deref()),
        Commands::Next { profile } => next_profile(&store, profile.as_deref()),
    }
}

fn list_profiles(store: &AutomationStore) {
    if store.profiles().is_empty() {
        println!("No profiles saved.");
        return;
    }
    for profile in store.profiles() {
        println!(
            "{}\t{}\tpresentation={:?}\tdisplays={}\tqueue={}\tschedule={}\tstill_set={}",
            profile.id.to_hyphenated_string(),
            profile.name,
            profile.presentation,
            profile.displays.len(),
            profile.rotation_queue_id.map_or_else(
                || "-".into(),
                easel_core::RotationQueueId::to_hyphenated_string
            ),
            profile
                .schedule_id
                .map_or_else(|| "-".into(), easel_core::ScheduleId::to_hyphenated_string),
            profile.still_set_id.map_or_else(
                || "-".into(),
                easel_core::DynamicStillSetId::to_hyphenated_string
            ),
        );
    }
}

fn list_stills(store: &AutomationStore) {
    if store.still_sets().is_empty() {
        println!("No dynamic still sets saved.");
        return;
    }
    for still_set in store.still_sets() {
        println!(
            "{}\t{}\tframes={}\tfallback={}\tcross_fade={}",
            still_set.id.to_hyphenated_string(),
            still_set.name,
            still_set.frames.len(),
            still_set.fallback_asset_id.to_hyphenated_string(),
            still_set.request_cross_fade
        );
        for frame in &still_set.frames {
            println!(
                "\t{}\t{}",
                frame.key.label(),
                frame.asset_id.to_hyphenated_string()
            );
        }
    }
}

fn list_schedules(store: &AutomationStore, utc_offset_minutes: i32) -> Result<(), String> {
    if store.schedules().is_empty() {
        println!("No schedules saved.");
        return Ok(());
    }
    let now = InstantSeconds {
        unix_seconds: now_unix_i64(),
    };
    for schedule in store.schedules() {
        let last = store
            .history()
            .last_fired(schedule.id)
            .map_err(|error| error.to_string())?
            .map(|unix_seconds| InstantSeconds { unix_seconds });
        let next = next_fire_after(schedule, now, last, utc_offset_minutes);
        let hint = next.map_or_else(
            || "never".into(),
            |instant| explain_fire(schedule, instant, utc_offset_minutes),
        );
        println!(
            "{}\t{}\tenabled={}\t{}",
            schedule.id.to_hyphenated_string(),
            schedule.name,
            schedule.enabled,
            hint
        );
    }
    Ok(())
}

fn show_status(store: &AutomationStore, utc_offset_minutes: i32) -> Result<(), String> {
    let summary = store
        .summary(utc_offset_minutes)
        .map_err(|error| error.to_string())?;
    println!("profiles:\t{}", summary.profile_count);
    println!("enabled schedules:\t{}", summary.enabled_schedules);
    println!("still sets:\t{}", summary.still_set_count);
    println!("paused:\t{}", summary.any_paused);
    println!(
        "next fire:\t{}",
        summary.next_fire_hint.as_deref().unwrap_or("none")
    );
    println!(
        "next dynamic:\t{}",
        summary.next_dynamic_hint.as_deref().unwrap_or("none")
    );
    println!(
        "last apply:\t{}",
        summary.last_apply_reason.as_deref().unwrap_or("none")
    );
    println!("hotplug:\t{}", summary.hotplug_policy);

    let now = InstantSeconds {
        unix_seconds: now_unix_i64(),
    };
    for profile in store.profiles() {
        if profile.presentation != easel_core::PresentationMode::DynamicStills {
            continue;
        }
        let Some(still_set_id) = profile.still_set_id else {
            continue;
        };
        let Some(still_set) = store.still_set(still_set_id) else {
            continue;
        };
        let selection = easel_core::active_frame_at(still_set, now, utc_offset_minutes);
        let last = store
            .history()
            .dynamic_still_state(profile.id)
            .map_err(|error| error.to_string())?;
        println!(
            "dynamic {}:\tactive={} ({}) last={}",
            profile.name,
            selection.key_label(),
            selection.asset_id.to_hyphenated_string(),
            last.as_ref()
                .map_or("none", |frame| frame.key_label.as_str())
        );
    }
    Ok(())
}

fn skip_profile(store: &mut AutomationStore, profile: Option<&str>) -> Result<(), String> {
    let profile_id = resolve_profile_id(store, profile)?;
    let membership = membership_for(store, profile_id)?;
    let (_skipped, reason) = store
        .skip_for_profile(profile_id, &membership)
        .map_err(|error| error.to_string())?;
    println!("{reason}");
    Ok(())
}

fn next_profile(store: &AutomationStore, profile: Option<&str>) -> Result<(), String> {
    let profile_id = resolve_profile_id(store, profile)?;
    let membership = membership_for(store, profile_id)?;
    let (queue_id, decision) = store
        .select_for_profile(profile_id, &membership)
        .map_err(|error| error.to_string())?;
    println!("queue:\t{}", queue_id.to_hyphenated_string());
    println!("asset:\t{}", decision.asset_id.to_hyphenated_string());
    println!("reason:\t{}", decision.reason);
    println!("next_cursor:\t{}", decision.next_cursor);
    Ok(())
}

fn open_store() -> Result<AutomationStore, String> {
    let (config_dir, data_dir) = dirs();
    AutomationStore::open(AutomationPaths::new(config_dir, data_dir))
        .map_err(|error| error.to_string())
}

fn dirs() -> (PathBuf, PathBuf) {
    ProjectDirs::from("net", "fritztech", "easel").map_or_else(
        || {
            (
                PathBuf::from(".").join("easel-config"),
                PathBuf::from(".").join("easel-data"),
            )
        },
        |dirs| {
            (
                dirs.config_dir().to_path_buf(),
                dirs.data_dir().to_path_buf(),
            )
        },
    )
}

fn resolve_profile_id(store: &AutomationStore, profile: Option<&str>) -> Result<ProfileId, String> {
    if let Some(value) = profile {
        return ProfileId::parse(value).map_err(|error| error.to_string());
    }
    store
        .profiles()
        .iter()
        .find(|profile| profile.rotation_queue_id.is_some())
        .map(|profile| profile.id)
        .ok_or_else(|| "no profile with a rotation queue; pass --profile".into())
}

fn membership_for(store: &AutomationStore, profile_id: ProfileId) -> Result<Vec<AssetId>, String> {
    let profile = store
        .profile(profile_id)
        .ok_or_else(|| format!("profile not found: {}", profile_id.to_hyphenated_string()))?;
    let queue_id = profile
        .rotation_queue_id
        .ok_or_else(|| "profile has no rotation queue".to_string())?;
    let queue = store
        .queue(queue_id)
        .ok_or_else(|| format!("queue not found: {}", queue_id.to_hyphenated_string()))?;
    match &queue.source {
        RotationSource::Assets { asset_ids } => Ok(asset_ids.clone()),
        RotationSource::Collection { collection_id } => Err(format!(
            "collection queues require the library store; unresolved collection {}",
            collection_id.to_hyphenated_string()
        )),
    }
}
