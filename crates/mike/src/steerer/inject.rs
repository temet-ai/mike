use super::rollout_params::SteererClass;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplyDemand {
    Required,
    None,
}

impl ReplyDemand {
    pub(crate) fn for_steerer(authority: &str, class: SteererClass) -> Self {
        if authority == "command" && class == SteererClass::Correction {
            Self::Required
        } else {
            Self::None
        }
    }
}

pub(crate) const REPLY_LINE: &str = "If it is already done or does not apply, say so in a line.";

pub(crate) fn build_steering_block(inner: &str) -> String {
    build_steering_block_with(inner, ReplyDemand::Required)
}

pub(crate) fn build_steering_block_with(inner: &str, reply: ReplyDemand) -> String {
    match reply {
        ReplyDemand::Required => format!(
            "<system-reminder>Steering note from the operator's proxy:\n\n{inner}\n\n{REPLY_LINE}</system-reminder>"
        ),
        ReplyDemand::None => format!(
            "<system-reminder>Steering note from the operator's proxy:\n\n{inner}</system-reminder>"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let reply = ReplyDemand::for_steerer("command", SteererClass::Efficiency);
        assert_eq!(reply, ReplyDemand::None);
        let block = build_steering_block_with("use ast-grep", reply);
        assert!(!block.contains(REPLY_LINE));
        assert!(block.ends_with("use ast-grep</system-reminder>"));
        let supply = ReplyDemand::for_steerer("supply_context", SteererClass::Correction);
        assert_eq!(supply, ReplyDemand::None);
    }

    #[test]
    fn command_correction_note_keeps_reply_demand() {
        let reply = ReplyDemand::for_steerer("command", SteererClass::Correction);
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
