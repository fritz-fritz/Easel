// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Headless CLI commands for automation control.

use std::process;

use easel_core::MissingOutputPolicy;

use crate::apply_service;
use crate::automation_session;

/// Parses and executes `--cli <command>` before the Qt event loop starts.
///
/// Returns `true` when a CLI command was handled (caller should exit).
pub fn maybe_run(args: &[String]) -> bool {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--cli" {
            let Some(command) = iter.next() else {
                eprintln!(
                    "usage: easel-desktop --cli <status|pause|resume|skip|apply-next|tick|set-interval|set-policy>"
                );
                process::exit(2);
            };
            let rest: Vec<String> = iter.cloned().collect();
            if let Err(error) = dispatch(command, &rest) {
                eprintln!("cli error: {error}");
                process::exit(1);
            }
            return true;
        }
        if let Some(command) = arg.strip_prefix("--cli=") {
            let rest: Vec<String> = iter.cloned().collect();
            if let Err(error) = dispatch(command, &rest) {
                eprintln!("cli error: {error}");
                process::exit(1);
            }
            return true;
        }
    }
    false
}

fn dispatch(command: &str, rest: &[String]) -> Result<(), String> {
    match command {
        "status" => {
            println!("{}", automation_session::status_summary()?);
            Ok(())
        }
        "pause" => {
            automation_session::set_paused(true)?;
            println!("paused");
            Ok(())
        }
        "resume" => {
            automation_session::set_paused(false)?;
            println!("resumed");
            Ok(())
        }
        "skip" | "apply-next" => {
            let message = apply_service::run_automation_tick(true)?;
            println!("{message}");
            Ok(())
        }
        "tick" => {
            let message = apply_service::run_automation_tick(false)?;
            println!("{message}");
            Ok(())
        }
        "set-interval" => {
            let seconds: u64 = rest
                .first()
                .ok_or_else(|| "set-interval requires seconds".to_owned())?
                .parse()
                .map_err(|error| format!("invalid seconds: {error}"))?;
            let schedule = automation_session::set_interval_schedule("CLI interval", seconds)?;
            println!(
                "active schedule {} every {}s",
                schedule.id.to_hyphenated_string(),
                seconds
            );
            Ok(())
        }
        "set-policy" => {
            let policy = match rest.first().map(String::as_str) {
                Some("skip-missing") => MissingOutputPolicy::SkipMissing,
                Some("pause-until-restored") => MissingOutputPolicy::PauseUntilRestored,
                Some("require-any") => MissingOutputPolicy::RequireAny,
                other => {
                    return Err(format!(
                        "unknown policy {other:?}; expected skip-missing|pause-until-restored|require-any"
                    ));
                }
            };
            automation_session::set_missing_output_policy(policy)?;
            println!("missing-output policy set to {policy:?}");
            Ok(())
        }
        other => Err(format!(
            "unknown cli command '{other}'; expected status|pause|resume|skip|apply-next|tick|set-interval|set-policy"
        )),
    }
}
