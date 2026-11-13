use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::{BuildWaits, add_to_record, build_waits};
use crate::launch::fleet::build_timing::Timing;

fn at(minute: i64) -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH + Duration::minutes(minute)
}

#[test]
fn a_command_splits_into_lock_wait_unit_wait_and_compile_time() {
    let waited = Timing {
        session: Uuid::nil(),
        program: None,
        args: "test -p mike --lib launch".into(),
        started_at: at(0),
        lock_acquired_at: Some(at(9)),
        admitted_at: Some(at(10)),
        finished_at: at(14),
        exit: 0,
    };
    let older = Timing {
        lock_acquired_at: None,
        admitted_at: Some(at(2)),
        finished_at: at(5),
        ..waited.clone()
    };
    let waits = build_waits(&[waited, older]);
    assert_eq!(
        waits,
        BuildWaits {
            commands: 2,
            lock_wait_minutes: 9.0,
            unit_wait_minutes: 3.0,
            compile_minutes: 7.0,
        }
    );
    let mut record = json!({"fault_minutes": {"resource_wait": 0.0, "stop": 1.5, "total": 1.5}});
    add_to_record(&mut record, waits);
    assert_eq!(record["fault_minutes"]["resource_wait"], json!(12.0));
    assert_eq!(record["fault_minutes"]["total"], json!(13.5));
    assert_eq!(record["build_waits"]["lock_wait_minutes"], json!(9.0));
}
