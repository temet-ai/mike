use super::rollout_params::SteererClass;
pub(crate) use crate::wire::steering_block::{ReplyDemand, build_steering_block_with};

pub(crate) fn reply_demand_for_steerer(authority: &str, class: SteererClass) -> ReplyDemand {
    if authority == "command" && class == SteererClass::Correction {
        ReplyDemand::Required
    } else {
        ReplyDemand::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::steering_block::{REPLY_LINE, build_steering_block};

    #[test]
    fn build_steering_block_wraps_inner() {
        let result = build_steering_block("hello");
        assert!(result.starts_with("<system-reminder>"));
        assert!(result.ends_with("</system-reminder>"));
        assert!(result.contains("hello"));
    }

    #[test]
    fn build_steering_block_declares_provenance_without_impersonating_user() {
        let result = build_steering_block("show the test output");
        assert!(result.contains("operator's proxy"));
        assert!(!result.contains("The user sent"));
        assert!(!result.contains("IMPORTANT"));
        assert!(!result.contains("MUST"));
    }

    #[test]
    fn advisory_note_has_no_reply_demand() {
        let reply = reply_demand_for_steerer("command", SteererClass::Efficiency);
        assert_eq!(reply, ReplyDemand::None);
        let block = build_steering_block_with("use ast-grep", reply);
        assert!(!block.contains(REPLY_LINE));
        assert!(block.ends_with("use ast-grep</system-reminder>"));
        let supply = reply_demand_for_steerer("supply_context", SteererClass::Correction);
        assert_eq!(supply, ReplyDemand::None);
    }

    #[test]
    fn command_correction_note_keeps_reply_demand() {
        let reply = reply_demand_for_steerer("command", SteererClass::Correction);
        assert_eq!(reply, ReplyDemand::Required);
        let block = build_steering_block_with("show the output", reply);
        assert!(block.ends_with(
            "show the output\n\nIf it is already done or does not apply, say so in a line.</system-reminder>"
        ));
        assert!(!block.contains("Act on it"));
    }

    #[test]
    fn build_steering_block_frame_stays_short_next_to_its_payload() {
        let inner = "show the test output";
        let frame_len = build_steering_block(inner).len() - inner.len();
        assert!(frame_len < 160, "steering frame grew to {frame_len} bytes");
    }
}
