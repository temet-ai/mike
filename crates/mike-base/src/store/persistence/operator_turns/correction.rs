pub const COUNTS_AS_CORRECTION_SQL: &str = "(toc.code = 'corrected' OR (toc.code = 'abandoned' AND t.outcome_source IS NOT DISTINCT FROM 'frustration_close'))";

pub fn counts_as_correction(outcome: &str, outcome_source: Option<&str>) -> bool {
    outcome == "corrected"
        || (outcome == "abandoned" && outcome_source == Some("frustration_close"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_as_correction_frustration_close_abandoned_counts() {
        assert!(counts_as_correction("abandoned", Some("frustration_close")));
        assert!(counts_as_correction("corrected", Some("live")));
        assert!(!counts_as_correction("abandoned", Some("live")));
        assert!(!counts_as_correction("accepted", Some("frustration_close")));
    }

    #[test]
    fn turn_select_base_uses_the_shared_expression() {
        assert!(super::super::TURN_SELECT_BASE.contains(COUNTS_AS_CORRECTION_SQL));
    }
}
