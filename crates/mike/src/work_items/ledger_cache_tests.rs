use chrono::{DateTime, Duration, TimeZone, Utc};

use super::ledger::{GitOutputRow, WorkItemRow};
use super::ledger_cache::{ActivityRow, GitDeltaRow, LedgerCache, ScopeRow, SessionReads};

fn at(minutes: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0)
        .single()
        .unwrap_or_default()
        + Duration::minutes(minutes)
}

fn agent_item(session: i64, uuid: &str, task: &str) -> ScopeRow {
    ScopeRow {
        session_db_id: session,
        session_uuid: uuid.to_owned(),
        item: WorkItemRow {
            kind: "agent".to_owned(),
            task_id: task.to_owned(),
            description: None,
            owner_agent_id: None,
            state: "running".to_owned(),
            launched_at: at(0),
            finished_at: None,
            exit_code: None,
            summary: None,
            last_request_at: None,
            request_count: 0,
            last_edit_at: None,
        },
    }
}

fn activity(session: i64, agent: Option<&str>, last: i64) -> ActivityRow {
    ActivityRow {
        session_db_id: session,
        agent_id: agent.map(str::to_owned),
        last_request_at: at(last),
        main_last_request_at: agent.is_none().then(|| at(last)),
        last_edit_at: None,
    }
}

#[test]
fn reads_after_first_read_everything_then_overlaps_the_last_read() {
    let mut cache = LedgerCache::default();
    cache.apply_scope(vec![agent_item(1, "s1", "a1")], None);
    assert_eq!(
        cache.reads_after(&[1]),
        vec![(1, DateTime::<Utc>::UNIX_EPOCH)]
    );
    cache.apply_reads(&SessionReads {
        activity: vec![activity(1, Some("a1"), 30)],
        git: Vec::new(),
    });
    assert_eq!(cache.reads_after(&[1]), vec![(1, at(20))]);
    cache.forget_reads(1);
    assert_eq!(
        cache.reads_after(&[1]),
        vec![(1, DateTime::<Utc>::UNIX_EPOCH)]
    );
}

#[test]
fn apply_reads_overlapping_rows_keep_the_latest_activity_and_each_result_once() -> anyhow::Result<()>
{
    let mut cache = LedgerCache::default();
    cache.apply_scope(vec![agent_item(1, "s1", "a1")], None);
    let push = GitDeltaRow {
        result_id: 9,
        session_db_id: 1,
        row: GitOutputRow {
            agent_id: Some("a1".to_owned()),
            at: at(25),
            text: "   abc1234..def5678  main -> main".to_owned(),
        },
    };
    let reads = SessionReads {
        activity: vec![activity(1, Some("a1"), 30), activity(1, None, 28)],
        git: vec![push.clone()],
    };
    cache.apply_reads(&reads);
    cache.apply_reads(&SessionReads {
        activity: vec![activity(1, Some("a1"), 10)],
        git: vec![push],
    });
    let ledger = cache
        .ledger(1, at(31))
        .ok_or_else(|| anyhow::anyhow!("session 1 is not cached"))?;
    assert_eq!(ledger.agents[0].last_request_at, Some(at(30)));
    assert_eq!(ledger.agents[0].git.pushes.len(), 1);
    assert_eq!(ledger.agents[0].state, "landed_polling");
    Ok(())
}

#[test]
fn apply_scope_drops_a_named_session_that_left_scope_and_keeps_the_rest() {
    let mut cache = LedgerCache::default();
    cache.apply_scope(
        vec![agent_item(1, "s1", "a1"), agent_item(2, "s2", "a2")],
        None,
    );
    cache.apply_scope(Vec::new(), Some(&["s1".to_owned()]));
    assert_eq!(cache.ids(), vec![2]);
    cache.apply_scope(vec![agent_item(3, "s3", "a3")], None);
    assert_eq!(cache.ids(), vec![3]);
    assert_eq!(cache.ids_of(&["s3".to_owned(), "s9".to_owned()]), vec![3]);
}

#[test]
fn next_change_is_when_a_running_agent_turns_silent() {
    let mut cache = LedgerCache::default();
    cache.apply_scope(vec![agent_item(1, "s1", "a1")], None);
    cache.apply_reads(&SessionReads {
        activity: vec![activity(1, Some("a1"), 5)],
        git: Vec::new(),
    });
    assert_eq!(cache.next_change(at(6)), Some(at(20)));
    assert_eq!(cache.next_change(at(21)), None);
}
