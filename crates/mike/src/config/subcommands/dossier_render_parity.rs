use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::DossierRenderArgs;
use crate::dossier::render::judge::JudgeSectionId;
use crate::dossier::render::{TypedView, render_inputs};
use crate::dossier::view::types::{AsOf, Audience, Budget, Needs};
use crate::store::persistence::PersistHandle;

#[derive(Debug, Deserialize)]
struct EInputRow {
    turn_id: i64,
    as_of: DateTime<Utc>,
    hist_asks: String,
    hist_directives: String,
    hist_corrections: String,
}

#[derive(Debug, Default, Serialize)]
struct SectionTally {
    equal: u64,
    different: u64,
    skipped: u64,
    first_differences: Vec<Difference>,
}

#[derive(Debug, Serialize)]
struct Difference {
    turn_id: i64,
    offset: usize,
    expected: String,
    actual: String,
}

const MAX_EXAMPLES: usize = 20;

fn window(text: &str, offset: usize) -> String {
    text.chars()
        .skip(offset.saturating_sub(10))
        .take(80)
        .collect()
}

fn first_difference(expected: &str, actual: &str) -> Option<usize> {
    let mut a = expected.chars();
    let mut b = actual.chars();
    let mut i = 0;
    loop {
        match (a.next(), b.next()) {
            (None, None) => return None,
            (x, y) if x != y => return Some(i),
            _ => i += 1,
        }
    }
}

fn compare(tally: &mut SectionTally, turn_id: i64, expected: &str, actual: &str) {
    match first_difference(expected, actual) {
        None => tally.equal += 1,
        Some(offset) => {
            tally.different += 1;
            if tally.first_differences.len() < MAX_EXAMPLES {
                tally.first_differences.push(Difference {
                    turn_id,
                    offset,
                    expected: window(expected, offset),
                    actual: window(actual, offset),
                });
            }
        }
    }
}

fn wanted(args: &DossierRenderArgs) -> Vec<String> {
    args.sections
        .as_deref()
        .unwrap_or("asks,directives,corrections")
        .split(',')
        .map(|s| s.trim().to_owned())
        .collect()
}

async fn judge_bodies(
    persist: &PersistHandle,
    row: &EInputRow,
) -> Result<Option<Vec<(JudgeSectionId, String)>>> {
    let Some(located) = persist.locate_turn(row.turn_id).await? else {
        return Ok(None);
    };
    let audience = Audience::Judge {
        turn_id: row.turn_id,
    };
    let inputs = crate::dossier::view::load(
        persist,
        located.session_db_id,
        AsOf::reconstructed_for_eval(row.as_of),
        Needs::for_audience(&audience),
    )
    .await?;
    let Ok(rendered) = render_inputs(&inputs, &audience, Budget::JudgeSlices) else {
        return Ok(None);
    };
    let TypedView::Judge(view) = rendered.typed else {
        return Ok(None);
    };
    Ok(Some(
        view.sections.into_iter().map(|s| (s.id, s.body)).collect(),
    ))
}

pub(super) async fn run(
    persist: &PersistHandle,
    path: &Path,
    args: &DossierRenderArgs,
) -> Result<()> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let rows: Vec<EInputRow> =
        serde_json::from_str(&text).context("the e-input file is not a JSON row array")?;
    let sections = wanted(args);
    let mut tallies: std::collections::BTreeMap<String, SectionTally> = sections
        .iter()
        .map(|s| (s.clone(), SectionTally::default()))
        .collect();
    for row in &rows {
        let bodies = judge_bodies(persist, row).await?;
        for (name, tally) in tallies.iter_mut() {
            let expected = match name.as_str() {
                "asks" => &row.hist_asks,
                "directives" => &row.hist_directives,
                "corrections" => &row.hist_corrections,
                _ => continue,
            };
            let actual = bodies.as_ref().and_then(|b| {
                b.iter()
                    .find(|(id, _)| id.code() == name.as_str())
                    .map(|(_, body)| body.clone())
            });
            match actual {
                Some(actual) => compare(tally, row.turn_id, expected, &actual),
                None => tally.skipped += 1,
            }
        }
    }
    let report = serde_json::json!({"rows": rows.len(), "sections": tallies});
    let rendered = serde_json::to_string_pretty(&report)?;
    if let Some(out) = &args.out {
        std::fs::write(out, &rendered).with_context(|| format!("writing {}", out.display()))?;
    }
    println!("{rendered}");
    let mismatches: u64 = tallies.values().map(|t| t.different).sum();
    if mismatches > 0 {
        bail!("{mismatches} section bodies differ from e");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::first_difference;

    #[test]
    fn first_difference_reports_the_character_offset() {
        assert_eq!(first_difference("abc", "abc"), None);
        assert_eq!(first_difference("abc", "abd"), Some(2));
        assert_eq!(first_difference("ab", "abc"), Some(2));
        assert_eq!(first_difference("é1", "é2"), Some(1));
    }
}
