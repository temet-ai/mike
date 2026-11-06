#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leg {
    Routed,
    Direct,
}

pub(super) fn is_versioned_api_path(path: &str) -> bool {
    path.starts_with("/v1/") || path.starts_with("/v2/")
}

pub fn decide(health_up: bool, path: &str) -> Leg {
    if health_up && is_versioned_api_path(path) {
        Leg::Routed
    } else {
        Leg::Direct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_and_v1_routes_routed() {
        assert_eq!(decide(true, "/v1/messages"), Leg::Routed);
        assert_eq!(decide(true, "/v1/complete"), Leg::Routed);
    }

    #[test]
    fn up_and_v2_routes_routed() {
        assert_eq!(decide(true, "/v2/messages"), Leg::Routed);
        assert_eq!(decide(true, "/v2/complete"), Leg::Routed);
    }

    #[test]
    fn up_and_non_api_routes_direct() {
        assert_eq!(decide(true, "/health"), Leg::Direct);
        assert_eq!(decide(true, "/"), Leg::Direct);
        assert_eq!(decide(true, "/v3/messages"), Leg::Direct);
    }

    #[test]
    fn down_routes_direct_even_for_v1_and_v2() {
        assert_eq!(decide(false, "/v1/messages"), Leg::Direct);
        assert_eq!(decide(false, "/v2/messages"), Leg::Direct);
    }

    #[test]
    fn pessimistic_start_unknown_routes_direct() {
        let health = crate::health::ProxyHealth::unknown();
        assert_eq!(decide(health.is_up(), "/v1/messages"), Leg::Direct);
    }

    #[test]
    fn is_versioned_api_path_matches_v1_and_v2() {
        assert!(is_versioned_api_path("/v1/messages"));
        assert!(is_versioned_api_path("/v1/complete"));
        assert!(is_versioned_api_path("/v2/messages"));
    }

    #[test]
    fn is_versioned_api_path_rejects_non_versioned_paths() {
        assert!(!is_versioned_api_path("/health"));
        assert!(!is_versioned_api_path("/"));
        assert!(!is_versioned_api_path("/v3/messages"));
        assert!(!is_versioned_api_path("/v1"));
        assert!(!is_versioned_api_path("/v10/messages"));
    }
}
