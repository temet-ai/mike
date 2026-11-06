pub(crate) fn is_well_formed_session_id(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_well_formed_session_id_canonical_uuid_returns_true() {
        assert!(is_well_formed_session_id(
            "550e8400-e29b-41d4-a716-446655440000"
        ));
    }

    #[test]
    fn is_well_formed_session_id_wrong_length_returns_false() {
        assert!(!is_well_formed_session_id("550e8400-e29b-41d4-a716-4466"));
        assert!(!is_well_formed_session_id(
            "550e8400-e29b-41d4-a716-446655440000-extra"
        ));
    }

    #[test]
    fn is_well_formed_session_id_missing_hyphens_returns_false() {
        assert!(!is_well_formed_session_id(
            "550e8400xe29bx41d4xa716x446655440000"
        ));
    }
}
