use super::*;

#[test]
fn backlog_below_the_cap_is_exact() {
    let backlog = LiveBacklog::from_capped(BACKLOG_COUNT_CAP, None);
    assert_eq!(backlog.eligible_without_verdict, BACKLOG_COUNT_CAP);
    assert!(!backlog.capped);
}

#[test]
fn backlog_one_past_the_cap_reports_the_cap_and_flags_it() {
    let backlog = LiveBacklog::from_capped(BACKLOG_COUNT_CAP + 1, None);
    assert_eq!(backlog.eligible_without_verdict, BACKLOG_COUNT_CAP);
    assert!(backlog.capped);
}

fn backlog_sql(closed_since: Option<DateTime<Utc>>) -> Result<String, PersistError> {
    let query = capped_backlog_query(1, closed_since, Utc::now())?;
    Ok(query.sql().as_str().to_owned())
}

#[test]
fn capped_backlog_query_limits_an_ordered_window() -> Result<(), PersistError> {
    let sql = backlog_sql(None)?;
    assert!(sql.contains("ORDER BY t.closed_at, t.id LIMIT $3"));
    assert!(sql.contains("AND t.closed_at < $2"));
    Ok(())
}

#[test]
fn capped_backlog_query_with_a_start_binds_it_before_the_end() -> Result<(), PersistError> {
    let sql = backlog_sql(Some(Utc::now()))?;
    assert!(sql.contains("AND t.closed_at >= $2 AND t.closed_at < $3"));
    assert!(sql.contains("LIMIT $4"));
    Ok(())
}

#[test]
fn capped_backlog_query_reads_the_judge_queue() -> Result<(), PersistError> {
    assert!(backlog_sql(None)?.contains("AND t.judge_queued_at IS NOT NULL"));
    Ok(())
}

#[test]
fn reconcile_query_queues_the_eligible_and_dequeues_the_rest() -> Result<(), PersistError> {
    let query = reconcile_queue_query(1)?;
    let sql = query.sql();
    let sql = sql.as_str();
    assert!(sql.contains("WHERE u.id = want.id AND u.judge_queued_at IS NULL"));
    assert!(sql.contains("AND NOT EXISTS (SELECT 1 FROM want WHERE want.id = u.id)"));
    assert!(!sql.contains("t.judge_queued_at IS NOT NULL"));
    Ok(())
}

#[test]
fn final_turns_sql_starts_from_the_reprojectable_predicate() {
    assert!(include_str!("final_turns.sql").contains(
        "AND (t.outcome_source IS NULL OR t.outcome_source IN ('judge', 'frustration_close'))"
    ));
}
