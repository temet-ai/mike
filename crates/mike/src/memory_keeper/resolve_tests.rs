use super::*;
use anyhow::{Result, bail};
use chrono::Duration;

const NOW_EPOCH_SECONDS: i64 = 1_794_225_600;
const ME: u128 = 1;

fn now() -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH + Duration::seconds(NOW_EPOCH_SECONDS)
}

fn session() -> SessionFacts<'static> {
    SessionFacts {
        session_uuid: Uuid::from_u128(ME),
        cwd: None,
        branch: None,
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
        linked_sessions: BTreeSet::new(),
        checkout_dirs: BTreeSet::new(),
    }
}

fn linked_thread(slug: &str, age_days: i64) -> ThreadFacts {
    let mut facts = thread(slug, slug, age_days);
    facts.linked_sessions.insert(Uuid::from_u128(ME));
    facts
}

fn run<'a>(
    prompt: Option<&'a str>,
    session: SessionFacts<'a>,
    threads: &'a [ThreadFacts],
) -> Resolution {
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
        Some("what is the status of alpha-rollout"),
        session(),
        &threads,
    ))?;
    assert_eq!(pick.slug, "alpha-rollout");
    assert!(pick.signals.contains(&Signal::ExplicitMention));
    Ok(())
}

#[test]
fn resolve_ticket_in_prompt_matches_thread_title_without_continuation() -> Result<()> {
    let threads = [
        thread("supplier-export", "PROJ-1529 supplier charge export", 12),
        thread("beta-cleanup", "Beta cleanup", 0),
    ];
    let pick = expect_one(run(Some("look into PROJ-1529 for me"), session(), &threads))?;
    assert_eq!(pick.slug, "supplier-export");
    assert!(!pick.signals.contains(&Signal::Deictic));
    Ok(())
}

#[test]
fn resolve_linked_session_with_continue_prompt_gives_one() -> Result<()> {
    let threads = [
        thread("beta-cleanup", "Beta cleanup", 0),
        linked_thread("alpha-rollout", 2),
    ];
    let pick = expect_one(run(
        Some("can we continue with the open epic"),
        session(),
        &threads,
    ))?;
    assert_eq!(pick.slug, "alpha-rollout");
    assert!(pick.signals.contains(&Signal::LinkedSession));
    assert!(pick.signals.contains(&Signal::Deictic));
    Ok(())
}

#[test]
fn resolve_checkout_directory_with_continue_prompt_gives_one() -> Result<()> {
    let mut here = thread("alpha-rollout", "Alpha rollout", 1);
    here.checkout_dirs.insert("/work/app/".to_owned());
    let threads = [thread("beta-cleanup", "Beta cleanup", 1), here];
    let mut facts = session();
    facts.cwd = Some("/work/app");
    let pick = expect_one(run(Some("carry on"), facts, &threads))?;
    assert_eq!(pick.slug, "alpha-rollout");
    assert!(pick.signals.contains(&Signal::CheckoutMatch));
    Ok(())
}

#[test]
fn resolve_feature_branch_match_counts_and_default_branch_does_not() -> Result<()> {
    let mut feature = thread("alpha-rollout", "Alpha rollout", 1);
    feature.branch = Some("feat/alpha".to_owned());
    let mut on_main = thread("beta-cleanup", "Beta cleanup", 1);
    on_main.branch = Some("main".to_owned());
    let threads = [on_main, feature];
    let mut facts = session();
    facts.branch = Some("feat/alpha");
    let pick = expect_one(run(Some("resume"), facts, &threads))?;
    assert_eq!(pick.slug, "alpha-rollout");
    let mut main_facts = session();
    main_facts.branch = Some("main");
    let none = run(Some("resume"), main_facts, &threads);
    assert_eq!(none, Resolution::None);
    Ok(())
}

#[test]
fn resolve_worktree_inside_session_cwd_counts() -> Result<()> {
    let mut tree = thread("alpha-rollout", "Alpha rollout", 1);
    tree.worktree = Some(".claude/worktrees/alpha/".to_owned());
    let threads = [thread("beta-cleanup", "Beta cleanup", 1), tree];
    let mut facts = session();
    facts.cwd = Some("/work/app/.claude/worktrees/alpha");
    let pick = expect_one(run(Some("pick up where we left off"), facts, &threads))?;
    assert!(pick.signals.contains(&Signal::WorktreeMatch));
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
    let pick = expect_one(run(Some("continue"), facts, &threads))?;
    assert_eq!(pick.slug, "beta-cleanup");
    assert!(pick.signals.contains(&Signal::RecentMention));
    Ok(())
}

#[test]
fn resolve_two_equally_linked_threads_give_several_by_slug() -> Result<()> {
    let threads = [
        linked_thread("zeta-work", 1),
        linked_thread("alpha-rollout", 1),
    ];
    let picks = expect_several(run(Some("where are we"), session(), &threads))?;
    let slugs: Vec<&str> = picks.iter().map(|pick| pick.slug.as_str()).collect();
    assert_eq!(slugs, vec!["alpha-rollout", "zeta-work"]);
    Ok(())
}

#[test]
fn resolve_two_threads_known_only_by_recency_abstain() {
    let threads = [
        thread("zeta-work", "Zeta work", 1),
        thread("alpha-rollout", "Alpha rollout", 1),
    ];
    assert_eq!(run(Some("continue"), session(), &threads), Resolution::None);
}

#[test]
fn resolve_the_only_recent_thread_with_continue_prompt_gives_one() -> Result<()> {
    let threads = [thread("alpha-rollout", "Alpha rollout", 0)];
    let pick = expect_one(run(
        Some("can we continue with the open epic"),
        session(),
        &threads,
    ))?;
    assert_eq!(pick.slug, "alpha-rollout");
    Ok(())
}

#[test]
fn resolve_more_than_three_close_threads_abstains() {
    let threads: Vec<ThreadFacts> = ["a-one", "b-two", "c-three", "d-four"]
        .iter()
        .map(|slug| linked_thread(slug, 1))
        .collect();
    assert_eq!(run(Some("carry on"), session(), &threads), Resolution::None);
}

#[test]
fn resolve_unrelated_prompt_abstains() {
    let threads = [
        linked_thread("alpha-rollout", 0),
        thread("beta-cleanup", "Beta cleanup", 0),
    ];
    let resolution = run(
        Some("rename the config field to timeout"),
        session(),
        &threads,
    );
    assert_eq!(resolution, Resolution::None);
}

#[test]
fn resolve_single_shared_word_without_continuation_abstains() {
    let threads = [thread("alpha-rollout", "Alpha rollout", 0)];
    assert_eq!(
        run(Some("fix the rollout script"), session(), &threads),
        Resolution::None
    );
}

#[test]
fn resolve_two_shared_words_without_continuation_resolves() -> Result<()> {
    let threads = [thread("alpha-rollout", "Alpha rollout plan", 0)];
    let pick = expect_one(run(
        Some("update the alpha rollout script"),
        session(),
        &threads,
    ))?;
    assert!(pick.signals.contains(&Signal::PromptWords));
    Ok(())
}

#[test]
fn resolve_stale_thread_drops_out_and_fresh_one_resolves() -> Result<()> {
    let threads = [
        thread("old-effort", "Old effort", 30),
        thread("new-effort", "New effort", 0),
    ];
    assert_eq!(
        expect_one(run(Some("where are we"), session(), &threads))?.slug,
        "new-effort"
    );
    Ok(())
}

#[test]
fn resolve_closed_thread_is_never_resolved_even_when_named() {
    let mut done = thread("alpha-rollout", "Alpha rollout", 0);
    done.status = "done".to_owned();
    let threads = [done];
    assert_eq!(
        run(Some("status of alpha-rollout"), session(), &threads),
        Resolution::None
    );
}

#[test]
fn resolve_no_prompt_with_one_linked_thread_gives_one() -> Result<()> {
    let threads = [
        thread("beta-cleanup", "Beta cleanup", 0),
        linked_thread("alpha-rollout", 3),
    ];
    let pick = expect_one(run(None, session(), &threads))?;
    assert_eq!(pick.slug, "alpha-rollout");
    Ok(())
}

#[test]
fn resolve_no_prompt_and_no_session_link_abstains() {
    let threads = [thread("alpha-rollout", "Alpha rollout", 0)];
    assert_eq!(run(None, session(), &threads), Resolution::None);
}

#[test]
fn resolve_no_threads_abstains() {
    assert_eq!(run(Some("continue"), session(), &[]), Resolution::None);
}

#[test]
fn resolve_stopword_only_overlap_abstains() {
    let mut stop = thread("alpha-rollout", "the and for with this that", 0);
    stop.goal = "what where when would should could".to_owned();
    let threads = [stop];
    let prompt = Some("the and for with this that what where when");
    assert_eq!(run(prompt, session(), &threads), Resolution::None);
}

#[test]
fn resolve_input_order_does_not_change_the_result() {
    let forward = [
        linked_thread("zeta-work", 1),
        linked_thread("alpha-rollout", 1),
        thread("mid-effort", "Mid effort", 2),
    ];
    let mut reversed = forward.to_vec();
    reversed.reverse();
    assert_eq!(
        run(Some("pick up where we left off"), session(), &forward),
        run(Some("pick up where we left off"), session(), &reversed)
    );
}
