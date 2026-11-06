use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use super::assets::TurnLine;

pub const R3_FROM: &str = "2026-08-31";

static PROBE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"(?i)Reply with (the single word|exactly|only)|Please greet me|disposable test session|verifying automated steering|^Say (hello|hi)\b",
    )
    .ok()
});

pub fn era_of(created_at: &str) -> &'static str {
    if created_at < R3_FROM {
        "r1-kimi"
    } else {
        "r3"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SyntheticReason {
    SkillEvalBatch,
    ScratchProject,
    ProbePrompt,
}

impl SyntheticReason {
    pub fn code(self) -> &'static str {
        match self {
            SyntheticReason::SkillEvalBatch => "skill_eval_batch",
            SyntheticReason::ScratchProject => "scratch_project",
            SyntheticReason::ProbePrompt => "probe_prompt",
        }
    }
}

pub fn synthetic(turns: &[TurnLine]) -> HashMap<i64, SyntheticReason> {
    let mut per_session: HashMap<&str, usize> = HashMap::new();
    for turn in turns {
        *per_session.entry(turn.session_uuid.as_str()).or_default() += 1;
    }
    let mut out = HashMap::new();
    for turn in turns {
        let path = turn.project_path.as_deref().unwrap_or("");
        let reason = if path.contains("temet-skills")
            && turn.created_at.starts_with("2026-07-27")
            && per_session
                .get(turn.session_uuid.as_str())
                .copied()
                .unwrap_or(0)
                <= 3
        {
            Some(SyntheticReason::SkillEvalBatch)
        } else if path.starts_with("/tmp/") || path.contains("/scratchpad") {
            Some(SyntheticReason::ScratchProject)
        } else if PROBE
            .as_ref()
            .is_some_and(|re| re.is_match(turn.user_prompt_head.as_deref().unwrap_or("")))
        {
            Some(SyntheticReason::ProbePrompt)
        } else {
            None
        };
        if let Some(reason) = reason {
            out.insert(turn.turn_id, reason);
        }
    }
    out
}

pub fn is_real(turn: &TurnLine, synthetic: &HashMap<i64, SyntheticReason>) -> bool {
    turn.own_prompt_kind.as_deref() == Some("genuine") && !synthetic.contains_key(&turn.turn_id)
}

pub type Stratum = (&'static str, String);

pub fn real_strata(
    turns: &[TurnLine],
    synthetic: &HashMap<i64, SyntheticReason>,
) -> BTreeMap<Stratum, usize> {
    let mut out = BTreeMap::new();
    for turn in turns.iter().filter(|t| is_real(t, synthetic)) {
        let key = (
            era_of(&turn.created_at),
            turn.stored_outcome.clone().unwrap_or_default(),
        );
        *out.entry(key).or_default() += 1;
    }
    out
}

pub const EXPECTED_STRATA: [(&str, &str, usize); 8] = [
    ("r1-kimi", "abandoned", 219),
    ("r1-kimi", "accepted", 954),
    ("r1-kimi", "corrected", 336),
    ("r1-kimi", "shipped", 206),
    ("r3", "abandoned", 407),
    ("r3", "accepted", 401),
    ("r3", "corrected", 32),
    ("r3", "shipped", 89),
];

pub const REAL_GT_PER_STRATUM: usize = 25;

pub fn check_strata(strata: &BTreeMap<Stratum, usize>) -> Result<(), String> {
    for (era, outcome, want) in EXPECTED_STRATA {
        let got = strata.get(&(era, outcome.to_owned())).copied().unwrap_or(0);
        if got != want {
            return Err(format!(
                "real stratum {era}/{outcome} has {got} turns, expected {want}"
            ));
        }
    }
    Ok(())
}

pub fn real_gt_by_stratum(
    gt_ids: &HashSet<i64>,
    turns: &[TurnLine],
    synthetic: &HashMap<i64, SyntheticReason>,
) -> BTreeMap<Stratum, usize> {
    let mut out = BTreeMap::new();
    for turn in turns
        .iter()
        .filter(|t| gt_ids.contains(&t.turn_id) && is_real(t, synthetic))
    {
        let key = (
            era_of(&turn.created_at),
            turn.stored_outcome.clone().unwrap_or_default(),
        );
        *out.entry(key).or_default() += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(id: i64, session: &str, created: &str, path: &str, head: &str) -> TurnLine {
        TurnLine {
            turn_id: id,
            session_uuid: session.to_owned(),
            created_at: created.to_owned(),
            project_path: Some(path.to_owned()),
            own_prompt_kind: Some("genuine".to_owned()),
            stored_outcome: Some("accepted".to_owned()),
            user_prompt_head: Some(head.to_owned()),
        }
    }

    #[test]
    fn eras_split_at_the_r3_date() {
        assert_eq!(era_of("2026-08-30T23:59:59Z"), "r1-kimi");
        assert_eq!(era_of("2026-08-31T00:00:00Z"), "r3");
    }

    #[test]
    fn synthetic_rules_follow_analyse_py() {
        let turns = vec![
            turn(1, "a", "2026-07-27T08:00:00Z", "/x/temet-skills", "hello"),
            turn(2, "b", "2026-09-01T00:00:00Z", "/tmp/demo", "hello"),
            turn(
                3,
                "c",
                "2026-09-01T00:00:00Z",
                "/repo",
                "Reply with exactly OK",
            ),
            turn(
                4,
                "d",
                "2026-09-01T00:00:00Z",
                "/repo",
                "refactor the store",
            ),
        ];
        let found = synthetic(&turns);
        assert_eq!(found.get(&1), Some(&SyntheticReason::SkillEvalBatch));
        assert_eq!(found.get(&2), Some(&SyntheticReason::ScratchProject));
        assert_eq!(found.get(&3), Some(&SyntheticReason::ProbePrompt));
        assert!(!found.contains_key(&4));
    }
}
