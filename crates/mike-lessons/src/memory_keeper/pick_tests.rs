use std::collections::BTreeSet;

use anyhow::Result;
use chrono::{Duration, TimeZone};
use uuid::Uuid;

use super::*;
use crate::llm::decisions::Answer;
use crate::memory_keeper::resolve::{SessionFacts, Signal};

fn candidates() -> Vec<Pick> {
    vec![
        Pick {
            slug: "migration-work".to_owned(),
            score: 0.9,
            signals: vec![Signal::OwnLatest],
        },
        Pick {
            slug: "ui-polish".to_owned(),
            score: 0.4,
            signals: vec![Signal::RecentCheckpoint],
        },
    ]
}

fn answered(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        QUESTION.to_owned(),
        Answer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), p)]),
            confidence: None,
        },
    )]))
}

fn thread(slug: &str) -> ThreadFacts {
    ThreadFacts {
        slug: slug.to_owned(),
        title: format!("{slug} title"),
        goal: "finish it".to_owned(),
        status: "implementing".to_owned(),
        branch: Some("feature".to_owned()),
        worktree: None,
        last_session_uuid: None,
        last_checkpoint_at: chrono::Utc.timestamp_opt(1_794_225_600, 0).unwrap()
            - Duration::days(2),
        linked_sessions: BTreeSet::new(),
        checkout_dirs: BTreeSet::new(),
    }
}

#[test]
fn compose_chosen_thread_above_the_floor_picks_that_thread() -> Result<()> {
    let picked = compose(&answered("ui-polish", 0.8), &candidates())
        .ok_or_else(|| anyhow::anyhow!("expected a decision"))?;
    assert_eq!(picked.applied, "chosen");
    assert!(matches!(picked.resolution, Resolution::One(ref p) if p.slug == "ui-polish"));
    Ok(())
}

#[test]
fn compose_none_choice_supplies_no_thread() -> Result<()> {
    let picked = compose(&answered(NONE_KEY, 0.7), &candidates())
        .ok_or_else(|| anyhow::anyhow!("expected a decision"))?;
    assert_eq!(picked.resolution, Resolution::None);
    assert_eq!(picked.applied, "no_thread");
    Ok(())
}

#[test]
fn compose_thread_below_the_floor_supplies_no_thread() -> Result<()> {
    let picked = compose(&answered("migration-work", 0.3), &candidates())
        .ok_or_else(|| anyhow::anyhow!("expected a decision"))?;
    assert_eq!(picked.resolution, Resolution::None);
    assert_eq!(picked.applied, "unsure");
    Ok(())
}

#[test]
fn compose_failed_answer_gives_the_off_path() {
    assert_eq!(compose(&Asked::failed_with("timeout"), &candidates()), None);
}

#[test]
fn compose_unknown_thread_gives_the_off_path() {
    assert_eq!(compose(&answered("invented", 0.9), &candidates()), None);
}

#[test]
fn state_carries_the_message_the_candidates_and_their_signals() {
    let threads = vec![thread("migration-work"), thread("ui-polish")];
    let input = ResolveInput {
        prompt: Some("what's the status of the migration"),
        session: SessionFacts {
            session_uuid: Uuid::from_u128(1),
            cwd: None,
            branch: None,
            recent_turn_text: vec!["older", "newer"],
        },
        threads: &threads,
        now: chrono::Utc.timestamp_opt(1_794_225_600, 0).unwrap(),
    };
    let value = state(&input, "what's the status of the migration", &candidates());
    assert_eq!(
        value["operator_message"],
        "what's the status of the migration"
    );
    assert_eq!(value["recent_turns_newest_first"][0], "newer");
    assert_eq!(value["candidate_threads"][0]["slug"], "migration-work");
    assert_eq!(
        value["candidate_threads"][0]["code_signals"][0],
        "OwnLatest"
    );
    assert_eq!(value["candidate_threads"][0]["days_since_checkpoint"], 2.0);
}

#[tokio::test]
async fn decide_without_a_decider_returns_the_code_resolution_unchanged() {
    let threads = vec![thread("migration-work")];
    let input = ResolveInput {
        prompt: Some("what's the status of the migration"),
        session: SessionFacts {
            session_uuid: Uuid::from_u128(1),
            cwd: None,
            branch: None,
            recent_turn_text: Vec::new(),
        },
        threads: &threads,
        now: chrono::Utc.timestamp_opt(1_794_225_600, 0).unwrap(),
    };
    let today = Resolution::One(candidates().remove(0));
    let decided = decide(None, &input, today.clone()).await;
    assert_eq!(decided.resolution, today);
    assert_eq!(decided.record_id, None);
}

#[test]
fn questions_offer_every_candidate_and_none() -> Result<()> {
    let threads = vec![thread("migration-work")];
    let asked = questions(&candidates(), &threads);
    let Some(Question::Choice { criteria, .. }) = asked.get(QUESTION) else {
        anyhow::bail!("expected a choice question");
    };
    let keys: Vec<&str> = criteria.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["migration-work", "none", "ui-polish"]);
    Ok(())
}
