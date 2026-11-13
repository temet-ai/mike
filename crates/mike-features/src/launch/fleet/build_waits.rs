#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BuildWaits {
    pub commands: usize,
    pub lock_wait_minutes: f64,
    pub unit_wait_minutes: f64,
    pub compile_minutes: f64,
}

fn minutes(from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>) -> f64 {
    ((to - from).as_seconds_f64() / 60.0).max(0.0)
}

pub fn build_waits(timings: &[super::build_timing::Timing]) -> BuildWaits {
    let mut waits = BuildWaits::default();
    for timing in timings {
        let locked = timing.lock_acquired_at.unwrap_or(timing.started_at);
        let admitted = timing.admitted_at.unwrap_or(locked);
        waits.commands += 1;
        waits.lock_wait_minutes += minutes(timing.started_at, locked);
        waits.unit_wait_minutes += minutes(locked, admitted);
        waits.compile_minutes += minutes(admitted, timing.finished_at);
    }
    waits
}

pub fn add_to_record(document: &mut serde_json::Value, waits: BuildWaits) {
    let round = |value: f64| (value * 10.0).round() / 10.0;
    let resource = round(waits.lock_wait_minutes + waits.unit_wait_minutes);
    document["build_waits"] = serde_json::json!({
        "commands": waits.commands,
        "lock_wait_minutes": round(waits.lock_wait_minutes),
        "unit_wait_minutes": round(waits.unit_wait_minutes),
        "compile_minutes": round(waits.compile_minutes),
    });
    if let Some(faults) = document
        .get_mut("fault_minutes")
        .and_then(serde_json::Value::as_object_mut)
    {
        let total = faults
            .get("total")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_default();
        faults.insert("resource_wait".into(), serde_json::json!(resource));
        faults.insert("total".into(), serde_json::json!(round(total + resource)));
    }
}

#[cfg(test)]
#[path = "build_waits_tests.rs"]
mod tests;
