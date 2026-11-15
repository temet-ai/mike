use super::definition::FleetDefinition;
use super::host_memory;
use super::status::FleetStatus;

pub fn live_worker_seats(status: &FleetStatus) -> usize {
    let closed: std::collections::BTreeSet<&str> = status
        .seats
        .iter()
        .filter(|seat| seat.closed)
        .map(|seat| seat.seat.as_str())
        .collect();
    status
        .tasks
        .iter()
        .filter_map(|task| task.seat.as_deref())
        .filter(|seat| !closed.contains(seat))
        .count()
}

pub fn binding_room(
    def: &FleetDefinition,
    (live, starting): (usize, usize),
    (surface_room, surfaces): (usize, String),
) -> (usize, String) {
    let seats = seat_room(def, live, starting);
    if seats < surface_room {
        let why = max_seats_reached(def, live, starting).unwrap_or(surfaces);
        return (seats, why);
    }
    (surface_room, surfaces)
}

pub fn max_seats_reached(def: &FleetDefinition, live: usize, starting: usize) -> Option<String> {
    let limit = def.staffing.max_seats?;
    (live + starting >= limit).then(|| {
        format!(
            "waiting: staffing.max_seats is {limit} and {live} worker seats are open, {starting} starting; a seat frees when its task's seat closes"
        )
    })
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
