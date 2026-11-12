//! The <system-reminder> envelope every steering note and diagnostic is sent in.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplyDemand {
    Required,
    None,
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
