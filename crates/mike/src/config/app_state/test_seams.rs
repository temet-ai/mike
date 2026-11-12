//! Entry points integration tests use to put the state into a shape a request
//! would; compiled only for tests and the `test-seams` feature.

use std::time::Instant;

pub fn register_in_flight_for_tests(state: &crate::config::AppState, session_id: String) -> u64 {
    let id = crate::store::models::RequestId::next();
    let record = crate::in_flight::InFlightRecord {
        session_id,
        started_at: Instant::now(),
        event_tx: state.event_tx.clone(),
    };
    let _ = state.in_flight.register(id, record);
    id.as_u64()
}
