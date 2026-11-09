use super::*;
use anyhow::{Result, bail};
use chrono::Duration;

const NOW_EPOCH_SECONDS: i64 = 1_794_225_600;

fn now() -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH + Duration::seconds(NOW_EPOCH_SECONDS)
}

fn session() -> SessionFacts<'static> {
    SessionFacts {
        session_uuid: Uuid::from_u128(1),
        predecessor_session_uuid: None,
        checkout_branch: None,
        checkout_worktree: None,
        recent_turn_text: Vec::new(),
    }
}

fn thread(slug: &str, title: &str, age_days: i64) -> ThreadFacts {
    ThreadFacts {
        slug: slug.to_owned(),
        title: title.to_owned(),
        goal: String::new(),
        status: "implementing".to_owned(),
        branch: None,
        worktree: None,
        last_session_uuid: None,
        last_checkpoint_at: now() - Duration::days(age_days),
    }
}

fn run<'a>(prompt: &'a str, session: SessionFacts<'a>, threads: &'a [ThreadFacts]) -> Resolution {
    resolve(&ResolveInput {
        prompt,
        session,
        threads,
        now: now(),
    })
}

fn expect_one(resolution: Resolution) -> Result<Pick> {
    match resolution {
        Resolution::One(pick) => Ok(pick),
        other => bail!("expected One, got {other:?}"),
    }
}

fn expect_several(resolution: Resolution) -> Result<Vec<Pick>> {
    match resolution {
        Resolution::Several(picks) => Ok(picks),
        other => bail!("expected Several, got {other:?}"),
    }
}

#[test]
fn resolve_slug_named_in_prompt_wins_over_fresher_thread() -> Result<()> {
    let threads = [
        thread("alpha-rollout", "Alpha rollout", 10),
        thread("beta-cleanup", "Beta cleanup", 0),
    ];
    let pick = expect_one(run(
        "what is the status of alpha-rollout",
        session(),
        &threads,
    ))?;
    assert_eq!(pick.slug, "alpha-rollout");
    assert!(pick.signals.contains(&Signal::ExplicitMention));
    Ok(())
}

#[test]
fn resolve_ticket_in_prompt_matches_thread_title_without_deictic_phrase() -> Result<()> {
    let threads = [
        thread("supplier-export", "WPCW-1529 supplier charge export", 12),
        thread("beta-cleanup", "Beta cleanup", 0),
    ];
    let pick = expect_one(run("look into WPCW-1529 for me", session(), &threads))?;
    assert_eq!(pick.slug, "supplier-export");
    assert!(pick.signals.contains(&Signal::ExplicitMention));
    assert!(!pick.signals.contains(&Signal::Deictic));
    Ok(())
}

#[test]
fn resolve_branch_match_with_deictic_prompt_wins() -> Result<()> {
    let mut on_branch = thread("alpha-rollout", "Alpha rollout", 1);
    on_branch.branch = Some("feat/alpha".to_owned());
    let other = thread("beta-cleanup", "Beta cleanup", 1);
    let mut facts = session();
    facts.checkout_branch = Some("feat/alpha".to_owned());
    let threads = [other, on_branch];
    let pick = expect_one(run("continue where we left off", facts, &threads))?;
    assert_eq!(pick.slug, "alpha-rollout");
    assert!(pick.signals.contains(&Signal::BranchMatch));
    assert!(pick.signals.contains(&Signal::Deictic));
    Ok(())
}

#[test]
fn resolve_worktree_match_ignores_trailing_slash() -> Result<()> {
    let mut on_tree = thread("alpha-rollout", "Alpha rollout", 1);
    on_tree.worktree = Some("/work/alpha/".to_owned());
    let other = thread("beta-cleanup", "Beta cleanup", 1);
    let mut facts = session();
    facts.checkout_worktree = Some("/work/alpha".to_owned());
    let threads = [other, on_tree];
    let pick = expect_one(run("carry on", facts, &threads))?;
    assert_eq!(pick.slug, "alpha-rollout");
    assert!(pick.signals.contains(&Signal::WorktreeMatch));
    Ok(())
}

#[test]
fn resolve_last_session_equal_to_predecessor_counts() -> Result<()> {
    let predecessor = Uuid::from_u128(7);
    let mut owned = thread("alpha-rollout", "Alpha rollout", 1);
    owned.last_session_uuid = Some(predecessor);
    let other = thread("beta-cleanup", "Beta cleanup", 1);
    let mut facts = session();
    facts.predecessor_session_uuid = Some(predecessor);
    let threads = [other, owned];
    let pick = expect_one(run("resume", facts, &threads))?;
    assert_eq!(pick.slug, "alpha-rollout");
    assert!(pick.signals.contains(&Signal::LastSession));
    Ok(())
}

#[test]
fn resolve_recent_turn_mention_lifts_thread_above_equally_fresh_peer() -> Result<()> {
    let threads = [
        thread("alpha-rollout", "Alpha rollout", 1),
        thread("beta-cleanup", "Beta cleanup", 1),
    ];
    let mut facts = session();
    facts.recent_turn_text = vec!["we were reviewing beta-cleanup earlier"];
    let pick = expect_one(run("continue", facts, &threads))?;
    assert_eq!(pick.slug, "beta-cleanup");
    assert!(pick.signals.contains(&Signal::RecentMention));
    Ok(())
}

#[test]
fn resolve_two_equal_threads_with_continue_prompt_gives_several_by_slug() -> Result<()> {
    let threads = [
        thread("zeta-work", "Zeta work", 1),
        thread("alpha-rollout", "Alpha rollout", 1),
    ];
    let picks = expect_several(run(
        "can we continue with the open epic",
        session(),
        &threads,
    ))?;
    let slugs: Vec<&str> = picks.iter().map(|pick| pick.slug.as_str()).collect();
    assert_eq!(slugs, vec!["alpha-rollout", "zeta-work"]);
    Ok(())
}

#[test]
fn resolve_many_equal_threads_caps_several_at_three() -> Result<()> {
    let threads: Vec<ThreadFacts> = ["a-one", "b-two", "c-three", "d-four", "e-five"]
        .iter()
        .map(|slug| thread(slug, "Same", 1))
        .collect();
    let picks = expect_several(run("carry on", session(), &threads))?;
    assert_eq!(picks.len(), 3);
    Ok(())
}

#[test]
fn resolve_unrelated_prompt_abstains() {
    let threads = [
        thread("alpha-rollout", "Alpha rollout", 0),
        thread("beta-cleanup", "Beta cleanup", 0),
    ];
    let resolution = run("rename the config field to timeout", session(), &threads);
    assert_eq!(resolution, Resolution::None);
}

#[test]
fn resolve_single_shared_word_without_deictic_phrase_abstains() {
    let threads = [thread("alpha-rollout", "Alpha rollout", 0)];
    let resolution = run("fix the rollout script", session(), &threads);
    assert_eq!(resolution, Resolution::None);
}

#[test]
fn resolve_two_shared_words_without_deictic_phrase_resolves() -> Result<()> {
    let threads = [thread("alpha-rollout", "Alpha rollout plan", 0)];
    let pick = expect_one(run("update the alpha rollout script", session(), &threads))?;
    assert_eq!(pick.slug, "alpha-rollout");
    assert!(pick.signals.contains(&Signal::PromptWords));
    Ok(())
}

#[test]
fn resolve_continuation_prompt_with_one_recent_thread_gives_one() -> Result<()> {
    let threads = [thread("alpha-rollout", "Alpha rollout", 0)];
    let pick = expect_one(run(
        "can we continue with the open epic",
        session(),
        &threads,
    ))?;
    assert_eq!(pick.slug, "alpha-rollout");
    assert!(pick.signals.contains(&Signal::RecentCheckpoint));
    assert!(pick.signals.contains(&Signal::Deictic));
    Ok(())
}

#[test]
fn resolve_stale_checkpoint_loses_to_fresh_one() -> Result<()> {
    let threads = [
        thread("old-effort", "Old effort", 30),
        thread("new-effort", "New effort", 0),
    ];
    let pick = expect_one(run("where are we", session(), &threads))?;
    assert_eq!(pick.slug, "new-effort");
    Ok(())
}

#[test]
fn resolve_continuation_prompt_with_only_stale_threads_abstains() {
    let threads = [thread("old-effort", "Old effort", 40)];
    assert_eq!(run("continue", session(), &threads), Resolution::None);
}

#[test]
fn resolve_continuation_prompt_without_threads_abstains() {
    assert_eq!(run("continue", session(), &[]), Resolution::None);
}

#[test]
fn resolve_stopword_only_overlap_does_not_resolve() {
    let mut stop = thread("alpha-rollout", "the and for with this that", 0);
    stop.goal = "what where when would should could".to_owned();
    let threads = [stop];
    let resolution = run(
        "the and for with this that what where when",
        session(),
        &threads,
    );
    assert_eq!(resolution, Resolution::None);
}

#[test]
fn resolve_result_is_independent_of_input_order() {
    let forward = [
        thread("zeta-work", "Zeta work", 1),
        thread("alpha-rollout", "Alpha rollout", 1),
        thread("mid-effort", "Mid effort", 2),
    ];
    let mut reversed = forward.to_vec();
    reversed.reverse();
    assert_eq!(
        run("pick up where we left off", session(), &forward),
        run("pick up where we left off", session(), &reversed)
    );
}

#[test]
fn tokenize_drops_stopwords_short_words_and_case() {
    let words: Vec<String> = tokenize("The Quick fox AND is on a Plan")
        .into_iter()
        .collect();
    assert_eq!(
        words,
        vec!["fox".to_owned(), "plan".to_owned(), "quick".to_owned()]
    );
}

#[test]
fn is_deictic_recognises_continuation_phrasing_on_word_boundaries() {
    for prompt in [
        "Can we continue?",
        "carry on please",
        "Where are we on this",
        "what\u{2019}s the status",
        "pick up the open epic",
        "I thought about that decision",
    ] {
        assert!(is_deictic(prompt), "{prompt}");
    }
    for prompt in ["discontinue the service", "pick upload", "rename the field"] {
        assert!(!is_deictic(prompt), "{prompt}");
    }
}

#[test]
fn tickets_extracts_project_style_keys_only() {
    assert_eq!(
        tickets("see WPCW-1529 and utf-x and a-1 plus ab-12."),
        vec!["wpcw-1529", "ab-12"]
    );
}
