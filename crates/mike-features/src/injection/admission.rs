pub const DUPLICATE_CONTENT: &str = "duplicate_content";
pub const ALREADY_IN_REQUEST: &str = "already_in_request";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    DeliverAndWake,
    DeliverAppendOnly,
    DeliverWakeThrottled,
    Suppressed(&'static str),
}

impl Admission {
    pub fn may_wake(self) -> bool {
        matches!(self, Admission::DeliverAndWake)
    }

    pub fn suppression_reason(self) -> Option<&'static str> {
        match self {
            Admission::Suppressed(reason) => Some(reason),
            Admission::DeliverAndWake
            | Admission::DeliverAppendOnly
            | Admission::DeliverWakeThrottled => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_deliver_and_wake_permits_a_wake() {
        assert!(Admission::DeliverAndWake.may_wake());
        assert!(!Admission::DeliverAppendOnly.may_wake());
        assert!(!Admission::DeliverWakeThrottled.may_wake());
        assert!(!Admission::Suppressed(DUPLICATE_CONTENT).may_wake());
    }

    #[test]
    fn only_suppressed_carries_a_reason() {
        assert_eq!(
            Admission::Suppressed(DUPLICATE_CONTENT).suppression_reason(),
            Some(DUPLICATE_CONTENT)
        );
        for admission in [
            Admission::DeliverAndWake,
            Admission::DeliverAppendOnly,
            Admission::DeliverWakeThrottled,
        ] {
            assert!(admission.suppression_reason().is_none());
        }
    }
}
