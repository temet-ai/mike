use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::full::resolutions;
use super::types::ReactionItem;
use super::types::{AsOfMode, CorrectionItem, CorrectionSource, DossierInputs};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::dossier_view::{CorrectionLabelRow, LiveVerdictRow};

pub const AUDIENCE_TIERS: [&str; 3] = ["gold", "relayed", "seed"];

pub fn corrections_from_labels(
    inputs: &DossierInputs,
    rows: &[CorrectionLabelRow],
) -> Vec<CorrectionItem> {
    let resolved = resolutions(inputs);
    let prompts: HashMap<i64, &str> = inputs
        .stream
        .iter()
        .map(|r| (r.request_id, r.text.as_str()))
        .collect();
    let mut out: Vec<CorrectionItem> = rows
        .iter()
        .filter_map(|row| {
            let next = resolved
                .get(&row.corrected_turn_id)?
                .next_genuine
                .as_ref()?;
            if next.at > inputs.as_of.at {
                return None;
            }
            Some(CorrectionItem {
                corrected_turn_id: row.corrected_turn_id,
                corrected_created_at: row.corrected_created_at,
                corrected_prompt: row.corrected_prompt.clone(),
                correcting_request_id: next.request_id,
                correcting_text: prompts
                    .get(&next.request_id)
                    .copied()
                    .unwrap_or("")
                    .to_owned(),
                correcting_at: next.at,
                source: CorrectionSource::Label {
                    label_id: row.label_id,
                    tier: row.tier.clone(),
                    split: row.split.clone(),
                    created_at: row.created_at,
                },
            })
        })
        .collect();
    out.sort_by_key(|c| (c.correcting_at, c.correcting_request_id));
    out
}

pub fn reactions_from_verdicts(rows: Vec<LiveVerdictRow>) -> Vec<ReactionItem> {
    rows.into_iter()
        .map(|row| ReactionItem {
            verdict_id: row.verdict_id,
            turn_id: row.turn_id,
            reaction: row.reaction,
            delivery: row.delivery,
            legacy_label: row.legacy_label,
            confidence: Some(f64::from(row.confidence)),
            rationale: Some(row.rationale),
            created_at: row.created_at,
            bundle_id: row.bundle_id,
        })
        .collect()
}

pub fn corrections_from_verdicts(inputs: &DossierInputs) -> Vec<CorrectionItem> {
    let resolved = resolutions(inputs);
    let prompts: HashMap<i64, &str> = inputs
        .stream
        .iter()
        .map(|r| (r.request_id, r.text.as_str()))
        .collect();
    let labelled: Vec<i64> = inputs
        .corrections
        .iter()
        .map(|c| c.corrected_turn_id)
        .collect();
    inputs
        .reactions
        .iter()
        .filter(|r| r.reaction == "corrected" && !labelled.contains(&r.turn_id))
        .filter_map(|r| {
            let turn = inputs.turns.iter().find(|t| t.turn_id == r.turn_id)?;
            let next = resolved.get(&r.turn_id)?.next_genuine.as_ref()?;
            if next.at > inputs.as_of.at {
                return None;
            }
            Some(CorrectionItem {
                corrected_turn_id: r.turn_id,
                corrected_created_at: turn.created_at,
                corrected_prompt: turn.prompt.clone(),
                correcting_request_id: next.request_id,
                correcting_text: prompts
                    .get(&next.request_id)
                    .copied()
                    .unwrap_or("")
                    .to_owned(),
                correcting_at: next.at,
                source: CorrectionSource::LiveVerdict {
                    verdict_id: r.verdict_id,
                    reaction: r.reaction.clone(),
                    created_at: r.created_at,
                },
            })
        })
        .collect()
}

pub fn label_bound(inputs: &DossierInputs) -> DateTime<Utc> {
    match inputs.as_of.mode {
        AsOfMode::Knowledge => inputs.as_of.at,
        AsOfMode::Reconstructed => {
            DateTime::<Utc>::from_timestamp(253_402_300_799, 0).unwrap_or(inputs.as_of.at)
        }
    }
}

pub async fn load_label_corrections(
    persist: &PersistHandle,
    inputs: &DossierInputs,
) -> Result<Vec<CorrectionItem>, crate::store::persistence::PersistError> {
    let Ok(session_uuid) = inputs.session.session_uuid.parse::<uuid::Uuid>() else {
        return Ok(Vec::new());
    };
    let tiers: Vec<String> = AUDIENCE_TIERS.iter().map(|t| (*t).to_owned()).collect();
    let rows = persist
        .correction_labels_as_of(session_uuid, label_bound(inputs), &tiers)
        .await?;
    Ok(corrections_from_labels(inputs, &rows))
}
