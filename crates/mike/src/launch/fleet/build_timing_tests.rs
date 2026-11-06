use super::*;

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + seconds, 0).unwrap_or_default()
}

fn row(
    use_id: i64,
    session: u128,
    from: i64,
    to: i64,
    background: bool,
    command: &str,
) -> SlowCommandRow {
    SlowCommandRow {
        use_id,
        seat: format!("dev-{session}"),
        session: Uuid::from_u128(session),
        started_at: at(from),
        finished_at: at(to),
        is_error: false,
        in_background: background,
        command: command.into(),
    }
}

fn timing(session: u128, args: &str, from: i64, to: i64, exit: i32) -> Timing {
    Timing {
        session: Uuid::from_u128(session),
        args: args.into(),
        started_at: at(from),
        lock_acquired_at: None,
        admitted_at: None,
        finished_at: at(to),
        exit,
    }
}

#[test]
fn a_foreground_command_takes_the_measured_cargo_time_not_the_tool_time() {
    let rows = [row(
        1,
        1,
        0,
        200,
        false,
        "timeout 580 cargo clippy -p gg-sessions; sleep 20",
    )];
    let recorded = [timing(1, "clippy -p gg-sessions", 1, 171, 0)];
    let (out, timings) = matched(&rows, &recorded, &[]);
    assert_eq!(out, rows.to_vec());
    assert_eq!(timings.len(), 1);
    assert_eq!(timings[0].use_id, 1);
    assert_eq!(timings[0].work, "cargo clippy -p gg-sessions");
    assert_eq!(timings[0].seconds, 170.0);
}

#[test]
fn a_background_command_is_completed_from_its_one_measured_run() {
    let rows = [row(2, 1, 0, 3, true, "cargo nextest run -p gg-core")];
    let recorded = [timing(1, "nextest run -p gg-core", 1, 186, 101)];
    let (out, timings) = matched(&rows, &recorded, &[]);
    assert!(!out[0].in_background);
    assert_eq!(out[0].finished_at, at(186));
    assert!(out[0].is_error);
    assert_eq!(timings[0].seconds, 185.0);
}

#[test]
fn runs_from_another_session_another_command_or_outside_the_call_are_not_matched() {
    let rows = [row(3, 1, 100, 300, false, "cargo build --bin gg")];
    for recorded in [
        timing(2, "build --bin gg", 101, 290, 0),
        timing(1, "build --bin other", 101, 290, 0),
        timing(1, "build --bin gg", 10, 90, 0),
        timing(1, "build --bin gg", 101, 400, 0),
    ] {
        let (out, timings) = matched(&rows, &[recorded], &[]);
        assert_eq!(out, rows.to_vec());
        assert!(timings.is_empty());
    }
}

#[test]
fn a_background_call_with_two_candidate_runs_stays_unmeasured() {
    let rows = [row(4, 1, 0, 3, true, "cargo test -p a")];
    let recorded = [
        timing(1, "test -p a", 1, 50, 0),
        timing(1, "test -p a", 2, 60, 0),
    ];
    let (out, timings) = matched(&rows, &recorded, &[]);
    assert!(out[0].in_background);
    assert!(timings.is_empty());
}

#[test]
fn a_wrapper_that_stands_for_cargo_matches_its_arguments() {
    let wrappers = vec!["scripts/cx".to_string()];
    let rows = [row(
        5,
        1,
        0,
        100,
        false,
        "scripts/cx nextest run -p mike --lib",
    )];
    let recorded = [timing(1, "nextest run -p mike --lib", 1, 90, 0)];
    let (_, timings) = matched(&rows, &recorded, &wrappers);
    assert_eq!(timings.len(), 1);
    assert_eq!(timings[0].seconds, 89.0);
}

#[test]
fn recorded_runs_are_read_back_per_run_and_a_bad_line_is_skipped() -> std::io::Result<()> {
    let dir = tempfile::tempdir()?;
    let first = timing(1, "check -p a", 0, 10, 0);
    append(dir.path(), "fleet-x/1", &first)?;
    std::fs::write(dir.path().join("fleet-y.jsonl"), "not json\n")?;
    append(dir.path(), "fleet-y", &timing(2, "test", 0, 5, 1))?;
    assert_eq!(read_run(dir.path(), "fleet-x/1"), vec![first]);
    assert_eq!(read_run(dir.path(), "fleet-y").len(), 1);
    assert!(read_run(dir.path(), "fleet-z").is_empty());
    Ok(())
}
