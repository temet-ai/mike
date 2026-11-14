use super::definition::FleetDefinition;
use super::host_memory;
use super::status::FleetStatus;

pub fn live_worker_seats(status: &FleetStatus) -> usize {
    status
        .tasks
        .iter()
        .filter(|task| task.seat.is_some())
        .count()
}

pub fn seat_room(def: &FleetDefinition, live: usize, starting: usize) -> usize {
    match def.staffing.max_seats {
        Some(limit) => limit.saturating_sub(live + starting),
        None => usize::MAX,
    }
}

pub fn memory_floor_gb(def: &FleetDefinition) -> Option<f64> {
    def.staffing.min_free_memory_gb.map(f64::from)
}

pub async fn memory_hold(def: &FleetDefinition) -> Option<String> {
    let floor = memory_floor_gb(def)?;
    let free = host_memory::free_gb().await;
    hold_text(floor, free)
}

pub fn hold_text(floor: f64, free: Option<f64>) -> Option<String> {
    if host_memory::allows(Some(floor), free) {
        return None;
    }
    Some(format!(
        "waiting: {:.1} GB of host memory free, the definition's staffing.min_free_memory_gb is {floor:.0}",
        free.unwrap_or_default()
    ))
}

#[cfg(test)]
#[path = "staff_limits_tests.rs"]
mod tests;
