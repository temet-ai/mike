use super::*;

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap_or_default()
}

fn ran(use_id: i64, seat: &str, started: &str, seconds: i64, command: &str) -> SlowCommandRow {
    SlowCommandRow {
        use_id,
        seat: seat.into(),
        session: Uuid::from_u128(u128::try_from(use_id).unwrap_or_default()),
        started_at: at(started),
        finished_at: at(started) + Duration::seconds(seconds),
        is_error: false,
        in_background: false,
        command: command.into(),
    }
}

#[test]
fn the_same_work_wrapped_differently_by_two_seats_is_a_repeat() {
    let slow = vec![
        ran(
            1,
            "dev-T1",
            "2026-10-01T10:00:00Z",
            100,
            "cd /repo && export DATABASE_URL=x && scripts/cx check -p mike --lib 2>&1 | tail -20",
        ),
        ran(
            2,
            "dev-T2",
            "2026-10-01T10:05:00Z",
            95,
            "cd /repo && (timeout 580 scripts/cx check -p mike --lib 2>&1 | grep -E '^error')",
        ),
        ran(
            3,
            "dev-T2",
            "2026-10-01T10:09:00Z",
            95,
            "scripts/cx check -p mike --lib",
        ),
        ran(
            4,
            "dev-T3",
            "2026-10-01T10:10:00Z",
            60,
            "cd /repo && cat a.rs; sleep 60",
        ),
        ran(
            5,
            "dev-T3",
            "2026-10-01T10:12:00Z",
            60,
            "scripts/cx nextest run -p mike --lib other",
        ),
    ];
    let found = measured_repeats(&slow, &[]);
    let pairs: Vec<(i64, i64)> = found
        .iter()
        .map(|r| (r.again.use_id, r.first.use_id))
        .collect();
    assert_eq!(pairs, vec![(2, 1), (3, 1)]);
    assert_eq!(found[0].work, "scripts/cx check -p mike --lib");
}

#[test]
fn a_build_run_through_the_projects_wrapper_matches_the_same_build_run_plainly() {
    let slow = vec![
        ran(1, "dev-T1", "2026-10-01T10:00:00Z", 60, "make check -p x"),
        ran(
            2,
            "dev-T2",
            "2026-10-01T10:05:00Z",
            60,
            "scripts/build-slot make check -p x 2>&1 | tail -5",
        ),
    ];
    assert!(measured_repeats(&slow, &[]).is_empty());
    let found = measured_repeats(&slow, &["scripts/build-slot".to_string()]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].work, "make check -p x");
}

#[test]
fn a_build_chained_after_an_edit_matches_by_its_build_part() {
    let slow = vec![
        ran(1, "dev-T1", "2026-10-01T10:00:00Z", 60, "make check -p x"),
        ran(
            2,
            "dev-T2",
            "2026-10-01T10:05:00Z",
            60,
            "python3 - <<'EOF'\nopen('a.rs', 'w').write('x')\nEOF\nsed -i s/a/b/ c.rs && make check -p x | tail -3",
        ),
    ];
    assert_eq!(measured_repeats(&slow, &[]).len(), 1);
}

#[test]
fn a_heredoc_that_only_writes_a_file_is_not_introduced() {
    let slow = vec![
        ran(1, "dev-T1", "2026-10-01T10:00:00Z", 60, "make check -p x"),
        ran(
            2,
            "dev-T2",
            "2026-10-01T10:05:00Z",
            60,
            "cat > notes.md <<'EOF'\nmake check -p x\nEOF",
        ),
    ];
    assert!(measured_repeats(&slow, &[]).is_empty());
}

#[test]
fn a_seat_repeating_itself_or_running_before_the_first_finished_is_not_a_repeat() {
    let slow = vec![
        ran(1, "dev-T1", "2026-10-01T10:00:00Z", 100, "make test"),
        ran(2, "dev-T1", "2026-10-01T10:02:00Z", 100, "make test"),
        ran(3, "dev-T2", "2026-10-01T10:01:00Z", 100, "make test"),
    ];
    assert!(measured_repeats(&slow, &[]).is_empty());
    let later = ran(4, "dev-T1", "2026-10-01T10:05:00Z", 100, "make test");
    let with_later: Vec<SlowCommandRow> = slow.into_iter().chain([later]).collect();
    let found = measured_repeats(&with_later, &[]);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].again.use_id, found[0].first.use_id), (4, 3));
}

#[test]
fn the_second_seat_receives_a_bounded_receipt_without_a_peer_mail_requirement() {
    let slow = vec![
        ran(1, "py-dev-T1", "2026-10-01T10:00:00Z", 60, "make test"),
        ran(
            2,
            "py-dev-T2",
            "2026-10-01T10:05:00Z",
            95,
            "cd /r && make test | tail",
        ),
    ];
    let found = measured_repeats(&slow, &[]);
    let body = mail(&found[0], &[]);
    assert!(body.starts_with(
        "[mike introduce] py-dev-T2 (session 00000000-0000-0000-0000-000000000002) spent 95 s"
    ));
    assert!(body.contains("No contact with the first seat is required"));
    assert!(!body.contains("Mail both"));
    assert!(!body.contains("first result already held"));
    assert!(!body.contains("and it passed"));
    let handoff = receipt::handoff(&found[0], &[]);
    assert_eq!(handoff["use_id"], 1);
    assert_eq!(handoff["still_valid"], serde_json::Value::Null);
    assert_eq!(handoff["commit"], serde_json::Value::Null);
    assert_eq!(handoff["full_command_and_output"]["find_use_id"], 1);
    assert_eq!(
        handoff["full_command_and_output"]["tool"],
        "list_session_tool_uses"
    );
    assert_eq!(
        handoff["full_command_and_output"]["arguments"]["include_results"],
        true
    );
}

#[test]
fn any_checkout_change_invalidates_cargo_work_outside_the_configured_check_paths() {
    for command in [
        "cargo build --bin gg",
        "cargo run --bin gg",
        "cargo nextest run -p gg",
    ] {
        let slow = vec![
            ran(1, "ink-T7", "2026-10-01T10:00:00Z", 60, command),
            ran(2, "core-T9", "2026-10-01T10:05:00Z", 60, command),
        ];
        let found = measured_repeats(&slow, &[]);
        for path in [
            "crates/gg-core/src/host.rs",
            "crates/gg-core/src/dock.rs",
            "crates/gg-sessions/src/lib.rs",
            "Cargo.lock",
            "docs/a.md",
        ] {
            let changed = [ChangedPathRow {
                path: path.into(),
                seen_at: at("2026-10-01T10:03:00Z"),
            }];
            assert!(
                !no_observed_invalidation(&changed, &found[0]),
                "{command}: {path}"
            );
            assert_eq!(receipt::handoff(&found[0], &changed)["still_valid"], false);
        }
    }
}

#[test]
fn an_edit_during_the_first_run_or_inside_the_second_command_means_the_first_result_did_not_hold() {
    let slow = vec![
        ran(1, "dev-T1", "2026-10-01T10:00:00Z", 60, "make test"),
        ran(2, "dev-T2", "2026-10-01T10:05:00Z", 60, "make test"),
    ];
    let found = measured_repeats(&slow, &[]);
    let change = |when: &str| {
        vec![ChangedPathRow {
            path: "src/a.py".into(),
            seen_at: at(when),
        }]
    };
    assert!(!no_observed_invalidation(
        &change("2026-10-01T10:00:30Z"),
        &found[0]
    ));
    assert!(!no_observed_invalidation(
        &change("2026-10-01T10:05:30Z"),
        &found[0]
    ));
    assert!(!no_observed_invalidation(
        &change("2026-10-01T10:06:30Z"),
        &found[0]
    ));
    assert!(no_observed_invalidation(
        &change("2026-09-30T23:00:00Z"),
        &found[0]
    ));
}

#[test]
fn changes_are_counted_once_per_path_between_the_two_runs() {
    let change = |path: &str, when: &str| ChangedPathRow {
        path: path.into(),
        seen_at: at(when),
    };
    let changed = vec![
        change("a.py", "2026-10-01T09:59:00Z"),
        change("a.py", "2026-10-01T10:01:00Z"),
        change("a.py", "2026-10-01T10:02:00Z"),
        change("b.py", "2026-10-01T10:04:00Z"),
        change("c.py", "2026-10-01T10:06:00Z"),
    ];
    assert_eq!(
        changed_between(
            &changed,
            at("2026-10-01T10:00:00Z"),
            at("2026-10-01T10:05:00Z")
        ),
        2
    );
}

pub(super) fn measured_repeats(slow: &[SlowCommandRow], wrappers: &[String]) -> Vec<Repeat> {
    let timings = slow
        .iter()
        .flat_map(|row| {
            work_through(&row.command, wrappers)
                .into_iter()
                .map(|work| CommandTiming {
                    use_id: row.use_id,
                    work,
                    seconds: (row.finished_at - row.started_at).num_seconds() as f64,
                })
        })
        .collect::<Vec<_>>();
    repeats_with_timings(slow, wrappers, &timings)
}
