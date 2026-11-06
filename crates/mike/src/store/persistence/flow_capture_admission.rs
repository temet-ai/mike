pub(crate) const FLOW_PRODUCER: &str = "mike";
pub(crate) const FLOW_HOOK_EVENT_NAME: &str = "capture";
pub(crate) const FLOW_START_SOURCE_LIVE: &str = "capture";
pub(crate) const FLOW_START_SOURCE_BACKFILL: &str = "backfill";

pub(crate) fn capture_admits(message_count: Option<i32>) -> bool {
    message_count == Some(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_admits_true_only_for_single_message() {
        assert!(capture_admits(Some(1)));
        assert!(!capture_admits(Some(0)));
        assert!(!capture_admits(Some(2)));
        assert!(!capture_admits(None));
    }
}
