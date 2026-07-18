// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Avoid-repeat and ordered/shuffle selection for rotation queues.

use thiserror::Error;

use easel_core::{AssetId, RotationQueue};

/// Successful next-asset selection with an explainable reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionOutcome {
    /// Chosen asset.
    pub asset_id: AssetId,
    /// Human-readable explanation.
    pub reason: String,
}

/// Failed selection.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SelectionError {
    /// Queue has no assets.
    #[error("rotation queue is empty")]
    EmptyQueue,
    /// Every asset is excluded by avoid-repeat history.
    #[error("no eligible assets remain after avoid-repeat filtering")]
    AllAvoided,
}

/// Selects the next asset from a validated queue.
///
/// `seed` is used only when `queue.shuffle` is true so CLI and GUI stay deterministic
/// when callers pass a stable seed (for example the current unix second).
pub fn select_next_asset(
    queue: &RotationQueue,
    recent: &[AssetId],
    seed: u64,
) -> Result<SelectionOutcome, SelectionError> {
    if queue.assets.is_empty() {
        return Err(SelectionError::EmptyQueue);
    }

    let avoid = usize::try_from(queue.avoid_repeat).unwrap_or(usize::MAX);
    let avoided: std::collections::HashSet<AssetId> = if avoid == 0 {
        std::collections::HashSet::new()
    } else {
        recent.iter().rev().take(avoid).copied().collect()
    };

    let eligible: Vec<AssetId> = queue
        .assets
        .iter()
        .copied()
        .filter(|asset| !avoided.contains(asset))
        .collect();

    let candidates = if eligible.is_empty() {
        // Soften avoid-repeat when it would stall the queue entirely.
        queue.assets.clone()
    } else {
        eligible
    };

    if candidates.is_empty() {
        return Err(SelectionError::AllAvoided);
    }

    let chosen = if queue.shuffle {
        let index = usize::try_from(seed).unwrap_or(0) % candidates.len();
        candidates[index]
    } else if let Some(current) = recent.last() {
        let position = queue
            .assets
            .iter()
            .position(|asset| asset == current)
            .unwrap_or(0);
        let mut next_index = (position + 1) % queue.assets.len();
        // Advance until we land on an eligible candidate, wrapping once.
        for _ in 0..queue.assets.len() {
            let candidate = queue.assets[next_index];
            if candidates.contains(&candidate) {
                break;
            }
            next_index = (next_index + 1) % queue.assets.len();
        }
        queue.assets[next_index]
    } else {
        candidates[0]
    };

    let reason = if queue.shuffle {
        format!(
            "selected shuffled asset {} of {} eligible (avoid_repeat={})",
            chosen.to_hyphenated_string(),
            candidates.len(),
            queue.avoid_repeat
        )
    } else {
        format!(
            "selected next ordered asset {} (avoid_repeat={})",
            chosen.to_hyphenated_string(),
            queue.avoid_repeat
        )
    };

    Ok(SelectionOutcome {
        asset_id: chosen,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::AssetId;

    fn queue(assets: Vec<AssetId>) -> RotationQueue {
        let mut queue = RotationQueue::new("Desk");
        queue.assets = assets;
        queue.avoid_repeat = 1;
        queue
    }

    #[test]
    fn advances_in_order() {
        let a = AssetId::new();
        let b = AssetId::new();
        let c = AssetId::new();
        let queue = queue(vec![a, b, c]);
        let outcome = select_next_asset(&queue, &[a], 0).unwrap();
        assert_eq!(outcome.asset_id, b);
    }

    #[test]
    fn avoids_recent_when_possible() {
        let a = AssetId::new();
        let b = AssetId::new();
        let mut queue = queue(vec![a, b]);
        queue.avoid_repeat = 1;
        let outcome = select_next_asset(&queue, &[a], 0).unwrap();
        assert_eq!(outcome.asset_id, b);
    }
}
