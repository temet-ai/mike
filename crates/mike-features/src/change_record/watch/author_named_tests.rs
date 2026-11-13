use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::{by_owner, from_named_calls, owns};
use crate::change_record::watch::author::Authorship;
use crate::launch::fleet::{FleetDefinition, definition::TaskDefinition};
use crate::store::persistence::change_record::NamedShellCall;

fn at(second: i64) -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH + Duration::seconds(second)
}

fn call(session: i64, tool_use_id: i64, started: i64) -> NamedShellCall {
    NamedShellCall {
        session_id: session,
        tool_use_id,
        started: at(started),
        finished: Some(at(started + 1)),
    }
}

#[test]
fn the_seat_whose_bash_call_names_the_path_is_credited_over_the_running_window() {
    assert_eq!(
        from_named_calls(&[call(1, 691_473, 10)]),
        Some(Authorship::Shell {
            session: 1,
            named_by: Some(691_473),
        })
    );
    assert_eq!(
        from_named_calls(&[call(1, 5, 10), call(1, 9, 20)]),
        Some(Authorship::Shell {
            session: 1,
            named_by: Some(9),
        })
    );
    assert_eq!(from_named_calls(&[call(1, 5, 10), call(4, 6, 10)]), None);
    assert_eq!(from_named_calls(&[]), None);
}

#[test]
fn a_window_only_shell_attribution_is_marked_inferred() {
    let inferred = Authorship::Shell {
        session: 4,
        named_by: None,
    };
    assert_eq!(inferred.detail()["inferred"], serde_json::json!(true));
    let named = Authorship::Shell {
        session: 1,
        named_by: Some(691_473),
    };
    assert_eq!(
        named.detail()["named_by_tool_use_id"],
        serde_json::json!(691_473)
    );
    assert!(named.detail().get("inferred").is_none());
}

#[test]
fn the_task_that_owns_the_path_ranks_its_seat_first() {
    let (t1, t4) = (Uuid::from_u128(1), Uuid::from_u128(4));
    let mut def = FleetDefinition::default();
    for (id, session, file) in [
        (
            "T1",
            t1,
            "crates/mike/tests/integration/fleet_run_record.rs",
        ),
        (
            "T4",
            t4,
            "crates/mike/src/launch/fleet/operation_admission.rs",
        ),
    ] {
        def.tasks.insert(
            id.into(),
            TaskDefinition {
                session: Some(session.to_string()),
                files: vec![file.into()],
                ..TaskDefinition::default()
            },
        );
    }
    let path = "crates/mike/tests/integration/fleet_run_record.rs";
    assert!(owns(&def, t1, path));
    assert!(!owns(&def, t4, path));
    assert_eq!(by_owner(&[(10, true), (40, false)]), Some(10));
    assert_eq!(by_owner(&[(10, true), (40, true)]), None);
    assert_eq!(by_owner(&[(10, false)]), None);
}
