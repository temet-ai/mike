use super::{InFlight, describe};

#[test]
fn describe_names_a_route_only_while_it_runs() {
    let first = InFlight::start("GET", "/test/pressure");
    let second = InFlight::start("GET", "/test/pressure");
    assert!(describe().contains("in flight 2x GET /test/pressure"));
    drop(first);
    assert!(describe().contains("in flight 1x GET /test/pressure"));
    drop(second);
    assert!(!describe().contains("/test/pressure"));
}
