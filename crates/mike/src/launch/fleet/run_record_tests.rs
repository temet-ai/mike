use serde_json::Value;

use super::run_record::{
    GIT_READ, MEASURE_VERSION, difference, median, operation_pattern, summarise,
};
use crate::store::persistence::fleet::{
    CargoMeasure, ForkMeasure, InterventionMeasure, MailPair, QuestionMeasure, ReportMeasure,
    RunMeasures, SeatMeasure, TimingMeasure, TokenMeasure,
};

fn seat(name: &str, kind: &str, calls: i64, reads: i64, first_commit: Option<f64>) -> SeatMeasure {
    SeatMeasure {
        seat: name.to_string(),
        kind: kind.to_string(),
        requests: 10,
        working_minutes: 4.04,
        waiting_minutes: 2.0,
        tool_calls: calls,
        git_reads: reads,
        cargo_commands: 1,
        minutes_to_first_commit: first_commit,
        calls_to_first_commit: first_commit.map(|_| 5),
        offered: vec!["Read".to_string(), "Bash".to_string()],
        used: vec!["Bash".to_string()],
    }
}

fn measures() -> RunMeasures {
    RunMeasures {
        seats: vec![
            seat("coordinator", "coordinator", 50, 5, None),
            seat("db-dev-T7", "worker", 100, 10, Some(6.0)),
            seat("rust-developer-T2", "worker", 100, 20, Some(10.0)),
            seat("tooling-T4", "worker", 100, 0, None),
        ],
        reports: ReportMeasure {
            reports: 4,
            answered: 3,
            median_minutes: Some(1.5),
            longest_minutes: Some(9.0),
            late_minutes: 2.5,
        },
        mail: vec![MailPair {
            from_seat: "a".to_string(),
            to_seat: "b".to_string(),
            messages: 2,
            reached: 2,
            acknowledged: 1,
        }],
        cargo: CargoMeasure {
            commands: 9,
            repeats: 3,
        },
        questions: QuestionMeasure {
            asked: 2,
            answered: 1,
        },
        forks: ForkMeasure {
            forks: 7,
            waited: 3,
        },
        timing: TimingMeasure {
            time_to_done_minutes: 100.0,
            stop_minutes: 20.0,
            relaunch_gap_minutes: 1.5,
            no_seat_minutes: 30.0,
            seat_launches: 5,
            hand_starts: 2,
            task_count: 3,
            worker_launches: 4,
        },
        tokens: vec![token("coordinator", 100), token("worker", 200)],
        interventions: InterventionMeasure {
            operator_turns: 4,
            mail_driven_turns: 9,
            operator_answers: 1,
            reports_filed: 6,
        },
    }
}

fn token(kind: &str, each: i64) -> TokenMeasure {
    TokenMeasure {
        kind: kind.to_string(),
        requests: 10,
        input_tokens: each,
        output_tokens: each,
        cache_read_tokens: each,
        cache_write_tokens: each,
    }
}

#[test]
fn the_four_numbers_are_stated_with_the_parts_that_make_them_up() {
    let doc = summarise(&measures());
    assert_eq!(MEASURE_VERSION, "run-record-v3");
    assert_eq!(doc["time_to_done_minutes"], 100.0);
    assert_eq!(doc["fault_minutes"]["no_seat"], 30.0);
    assert_eq!(doc["fault_minutes"]["relaunch_gap"], 1.5);
    assert_eq!(doc["fault_minutes"]["report_to_verdict_wait"], 2.5);
    assert_eq!(doc["fault_minutes"]["stop"], 20.0);
    assert_eq!(doc["fault_minutes"]["control_seat_model_time"], 4.0);
    assert_eq!(doc["fault_minutes"]["total"], 58.0);
}

#[test]
fn the_token_and_intervention_numbers_are_stated_with_their_parts() {
    let doc = summarise(&measures());
    assert_eq!(doc["tokens_per_accepted_task"]["accepted_tasks"], 3);
    assert_eq!(doc["tokens_per_accepted_task"]["all_tokens"], 1200);
    assert_eq!(doc["tokens_per_accepted_task"]["total"], 400.0);
    assert_eq!(
        doc["tokens_per_accepted_task"]["by_seat_kind"]["worker"]["total_tokens"],
        800
    );
    assert_eq!(doc["manual_interventions"]["total"], 7);
    assert_eq!(doc["manual_interventions"]["mail_driven_turns"], 9);
    assert_eq!(doc["manual_interventions"]["hand_starts"], 2);
}

#[test]
fn the_counts_explain_relaunches_replacements_and_unread_mail() {
    let doc = summarise(&measures());
    assert_eq!(doc["counts"]["seats_per_task"], 1.3);
    assert_eq!(doc["counts"]["relaunches"], 1);
    assert_eq!(doc["counts"]["context_replacements"], 0);
    assert_eq!(doc["counts"]["unread_messages_at_close"], 1);
    assert_eq!(doc["counts"]["problems_opened"], 0);
    assert!(
        doc["unmeasured"]
            .as_array()
            .is_some_and(|names| names.iter().any(|n| n == "check_timeout_minutes"))
    );
}

#[test]
fn a_run_with_no_tasks_has_no_token_rate() {
    let mut empty = measures();
    empty.timing.task_count = 0;
    let doc = summarise(&empty);
    assert_eq!(doc["tokens_per_accepted_task"]["total"], Value::Null);
    assert_eq!(doc["counts"]["seats_per_task"], Value::Null);
}

#[test]
fn the_difference_subtracts_numbers_and_skips_what_is_not_shared() {
    let now = serde_json::json!({"a": 10.5, "b": {"c": 3, "d": "x"}, "e": 1});
    let before = serde_json::json!({"a": 4.0, "b": {"c": 1, "d": "y"}});
    assert_eq!(
        difference(&now, &before),
        serde_json::json!({"a": 6.5, "b": {"c": 2.0}})
    );
}

#[test]
fn the_record_totals_the_seats_and_states_the_run_wide_measures() {
    let doc = summarise(&measures());
    assert_eq!(doc["measure_version"], MEASURE_VERSION);
    assert_eq!(doc["seat_count"], 4);
    assert_eq!(doc["worker_count"], 3);
    assert_eq!(doc["working_minutes"], 16.2);
    assert_eq!(doc["waiting_minutes"], 8.0);
    assert_eq!(doc["first_commit_median_minutes"], 8.0);
    assert_eq!(doc["first_commit_longest_minutes"], 10.0);
}

#[test]
fn the_record_totals_the_workers_calls_reads_cargo_questions_and_forks() {
    let doc = summarise(&measures());
    assert_eq!(doc["reports"], 4);
    assert_eq!(doc["report_to_verdict_longest_minutes"], 9.0);
    assert_eq!(doc["worker_tool_calls"], 300);
    assert_eq!(doc["worker_git_reads"], 30);
    assert_eq!(doc["git_reads_per_100_calls"], 10.0);
    assert_eq!(doc["operation_commands"], 9);
    assert_eq!(doc["operation_repeats_across_seats"], 3);
    assert_eq!(doc["questions_asked"], 2);
    assert_eq!(doc["questions_answered"], 1);
    assert_eq!(doc["forks"], 7);
    assert_eq!(doc["forks_waited"], 3);
}

#[test]
fn the_record_keeps_every_seat_the_mail_by_pair_and_the_tools() {
    let doc = summarise(&measures());
    assert_eq!(doc["seats"].as_array().map(Vec::len), Some(4));
    assert_eq!(doc["seats"][1]["seat"], "db-dev-T7");
    assert_eq!(doc["seats"][1]["minutes_to_first_commit"], 6.0);
    assert_eq!(doc["mail_by_pair"][0]["messages"], 2);
    assert_eq!(doc["tools_offered"], serde_json::json!(["Bash", "Read"]));
    assert_eq!(doc["tools_used"], serde_json::json!(["Bash"]));
}

#[test]
fn a_run_with_no_worker_calls_has_no_git_read_rate_and_no_first_commit() {
    let mut empty = measures();
    empty.seats = vec![seat("coordinator", "coordinator", 5, 1, None)];
    let doc = summarise(&empty);
    assert_eq!(doc["git_reads_per_100_calls"], Value::Null);
    assert_eq!(doc["first_commit_median_minutes"], Value::Null);
    assert_eq!(doc["first_commit_longest_minutes"], Value::Null);
    assert_eq!(doc["worker_count"], 0);
}

#[test]
fn the_median_takes_the_middle_or_the_mean_of_the_two_middle_values() {
    assert_eq!(median(&[]), None);
    assert_eq!(median(&[3.0]), Some(3.0));
    assert_eq!(median(&[9.0, 1.0, 5.0]), Some(5.0));
    assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
}

#[test]
fn declared_operations_are_counted_without_language_inference() -> anyhow::Result<()> {
    use super::definition::FleetDefinition;
    let def: FleetDefinition =
        serde_json::from_value(serde_json::json!({"execution": {"operations": {
            "web": {"command": "npm run typecheck", "purpose": "types"},
            "python": {"command": "python3 -m unittest", "purpose": "unit tests"},
            "docs": {"command": "scripts/check-links", "purpose": "links"}
        }}}))?;
    let pattern = regex::Regex::new(&operation_pattern(&def))?;
    for command in [
        "npm run typecheck",
        "python3 -m unittest",
        "scripts/check-links",
    ] {
        assert!(pattern.is_match(command));
    }
    assert!(!pattern.is_match("cargo test"));
    assert!(!pattern.is_match("echo npm run typecheck"));
    assert!(!regex::Regex::new(&operation_pattern(&FleetDefinition::default()))?.is_match(""));
    Ok(())
}

#[test]
fn the_git_read_pattern_is_a_regular_expression_the_database_can_use() {
    assert!(GIT_READ.contains("status|log|diff"));
}
