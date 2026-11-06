use super::*;

fn command(use_id: i64, minutes: i64, command: &str) -> SlowCommandRow {
    let started_at =
        DateTime::from_timestamp(1_700_000_000, 0).unwrap_or_default() + Duration::minutes(minutes);
    SlowCommandRow {
        use_id,
        seat: format!("dev-{use_id}"),
        session: Uuid::from_u128(u128::try_from(use_id).unwrap_or_default()),
        started_at,
        finished_at: started_at + Duration::seconds(185),
        is_error: false,
        in_background: false,
        command: command.into(),
    }
}

fn timing(use_id: i64, work: &str, seconds: f64) -> CommandTiming {
    CommandTiming {
        use_id,
        work: work.into(),
        seconds,
    }
}

#[test]
fn parent_tool_wall_time_does_not_prove_a_slow_command() {
    for command_text in [
        "cargo clippy -p gg-sessions --all-features",
        "git log --oneline -12 | cat; git status --short",
    ] {
        let commands = [command(1, 0, command_text), command(2, 5, command_text)];
        assert!(repeats(&commands, &[]).is_empty());
    }
}

#[test]
fn both_relevant_commands_need_finite_measured_durations_of_at_least_five_seconds() {
    let work = "cargo clippy -p gg-sessions --all-features";
    let commands = [command(1, 0, work), command(2, 5, work)];
    for seconds in [0.0, 0.5, 4.999, -1.0, f64::NAN, f64::INFINITY] {
        for (first, again) in [(seconds, 185.0), (170.0, seconds)] {
            let timings = [timing(1, work, first), timing(2, work, again)];
            assert!(repeats_with_timings(&commands, &[], &timings).is_empty());
        }
    }
    assert!(repeats_with_timings(&commands, &[], &[timing(2, work, 185.0)]).is_empty());
    let found = repeats_with_timings(
        &commands,
        &[],
        &[timing(1, work, 5.0), timing(2, work, 5.0)],
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].again_seconds, 5.0);
}

#[test]
fn compound_command_timing_is_not_attributed_to_its_build_subcommand() {
    let work = "cargo build --bin gg";
    let compound = "cargo build --bin gg; sleep 180";
    let commands = [command(1, 0, compound), command(2, 5, work)];
    let timings = [timing(1, compound, 185.0), timing(2, work, 5.0)];
    assert!(repeats_with_timings(&commands, &[], &timings).is_empty());
    let timings = [timing(1, work, 0.5), timing(2, work, 5.0)];
    assert!(repeats_with_timings(&commands, &[], &timings).is_empty());
}

#[test]
fn timing_for_another_use_or_work_cannot_certify_the_matching_command() {
    let work = "cargo build --bin gg";
    let commands = [command(1, 0, work), command(2, 5, work)];
    for first in [timing(3, work, 20.0), timing(1, "cargo test", 20.0)] {
        assert!(repeats_with_timings(&commands, &[], &[first, timing(2, work, 5.0)]).is_empty());
    }
}

#[test]
fn a_handoff_remains_bounded_with_full_receipt_retrieval_and_honest_unknowns() {
    let command_text = format!("cargo test {}", "--test long-name ".repeat(1000));
    let commands = [command(1, 0, &command_text), command(2, 5, &command_text)];
    let work = work_through(&command_text, &[]).remove(0);
    let found = repeats_with_timings(
        &commands,
        &[],
        &[timing(1, &work, 170.0), timing(2, &work, 185.0)],
    );
    let body = mail(&found[0], &[]);
    assert!(body.len() < 2500);
    let receipt = receipt::handoff(&found[0], &[]);
    assert_eq!(receipt["command_truncated"], true);
    assert!(receipt["still_valid"].is_null());
    assert!(receipt["commit"].is_null());
    assert_eq!(receipt["full_command_and_output"]["find_use_id"], 1);
    assert_eq!(
        receipt["full_command_and_output"]["arguments"]["id"],
        commands[0].session.to_string()
    );
    assert!(body.contains("No contact with the first seat is required"));
}

#[test]
fn a_failed_tool_receipt_is_preserved_without_inventing_a_test_verdict() {
    let work = "cargo test";
    let mut first = command(1, 0, work);
    first.is_error = true;
    let commands = [first, command(2, 5, work)];
    let found = repeats_with_timings(
        &commands,
        &[],
        &[timing(1, work, 10.0), timing(2, work, 10.0)],
    );
    let receipt = receipt::handoff(&found[0], &[]);
    assert_eq!(receipt["is_error"], true);
    assert_eq!(receipt["tool_outcome"], "tool reported an error");
    assert!(!mail(&found[0], &[]).contains("and it passed"));
}
