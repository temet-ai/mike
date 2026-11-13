use std::time::Instant;

pub fn register_in_flight_for_tests(state: &crate::state::AppState, session_id: String) -> u64 {
    let id = crate::store::models::RequestId::next();
    let record = crate::in_flight::InFlightRecord {
        session_id,
        started_at: Instant::now(),
        event_tx: state.event_tx.clone(),
    };
    let _ = state.in_flight.register(id, record);
    id.as_u64()
}
