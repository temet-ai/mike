use crate::extract::request::TurnOrigin;

pub(crate) fn should_classify(
    turn_origin: &TurnOrigin,
    is_count_tokens_probe: bool,
    mutation_exempt: bool,
) -> bool {
    matches!(turn_origin, TurnOrigin::PrimaryFirst) && !is_count_tokens_probe && !mutation_exempt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_classify_primary_first_not_probe_true() {
        assert!(should_classify(&TurnOrigin::PrimaryFirst, false, false));
    }

    #[test]
    fn should_classify_primary_first_probe_false() {
        assert!(!should_classify(&TurnOrigin::PrimaryFirst, true, false));
    }

    #[test]
    fn should_classify_non_primary_first_false() {
        assert!(!should_classify(&TurnOrigin::Primary, false, false));
        assert!(!should_classify(&TurnOrigin::Subagent, false, false));
    }

    #[test]
    fn should_classify_mutation_exempt_primary_first_false() {
        assert!(!should_classify(&TurnOrigin::PrimaryFirst, false, true));
    }
}
