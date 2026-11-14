use std::time::Duration;

use crate::store::models::ReminderSource;

pub const STEERER_WAKE_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatRule {
    OncePerSessionPerContent,
    Always,
    WhenAbsentFromRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InjectionPolicy {
    pub source_kind: &'static str,
    pub repeat: RepeatRule,
    pub min_wake_interval: Option<Duration>,
    pub may_wake: bool,
}

pub fn policy_for(source: &ReminderSource) -> InjectionPolicy {
    match source {
        ReminderSource::Steerer { .. } => InjectionPolicy {
            source_kind: "steerer",
            repeat: RepeatRule::Always,
            min_wake_interval: Some(STEERER_WAKE_INTERVAL),
            may_wake: true,
        },
        ReminderSource::SessionMailbox { .. } => InjectionPolicy {
            source_kind: "mailbox",
            repeat: RepeatRule::Always,
            min_wake_interval: None,
            may_wake: true,
        },
        ReminderSource::Lesson => proxy_authored_context("lesson"),
        ReminderSource::Recall => proxy_authored_context("recall"),
        ReminderSource::Diagnostic => proxy_authored_context("diagnostic"),
        ReminderSource::SelfIdentity => InjectionPolicy {
            source_kind: "self_identity",
            repeat: RepeatRule::WhenAbsentFromRequest,
            min_wake_interval: None,
            may_wake: false,
        },
        ReminderSource::WorkspaceBriefing => proxy_authored_context("workspace_briefing"),
        ReminderSource::CodeFindings => InjectionPolicy {
            source_kind: "code_findings",
            repeat: RepeatRule::Always,
            min_wake_interval: None,
            may_wake: true,
        },
    }
}

fn proxy_authored_context(source_kind: &'static str) -> InjectionPolicy {
    InjectionPolicy {
        source_kind,
        repeat: RepeatRule::OncePerSessionPerContent,
        min_wake_interval: None,
        may_wake: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn steerer() -> ReminderSource {
        ReminderSource::Steerer {
            kind: "verifier".to_string(),
            intervention_uuid: Uuid::new_v4().to_string(),
            advisory: false,
        }
    }

    fn mailbox() -> ReminderSource {
        ReminderSource::SessionMailbox {
            message_uuid: Uuid::new_v4(),
            from_session_uuid: None,
        }
    }

    #[test]
    fn proxy_authored_sources_never_wake_and_deliver_once_per_content() {
        for source in [
            ReminderSource::Lesson,
            ReminderSource::Recall,
            ReminderSource::Diagnostic,
            ReminderSource::WorkspaceBriefing,
        ] {
            let policy = policy_for(&source);
            assert!(
                !policy.may_wake,
                "{source:?} is proxy-authored context and must ride the request body"
            );
            assert_eq!(policy.repeat, RepeatRule::OncePerSessionPerContent);
            assert!(policy.min_wake_interval.is_none());
        }
    }

    #[test]
    fn self_identity_never_wakes_and_reasserts_when_absent_from_the_request() {
        let policy = policy_for(&ReminderSource::SelfIdentity);
        assert!(
            !policy.may_wake,
            "self-identity is proxy-authored context and must ride the request body"
        );
        assert_eq!(policy.repeat, RepeatRule::WhenAbsentFromRequest);
        assert!(policy.min_wake_interval.is_none());
    }

    #[test]
    fn peer_framed_sources_may_wake_and_are_always_deliverable() {
        for source in [steerer(), mailbox()] {
            let policy = policy_for(&source);
            assert!(policy.may_wake, "{source:?} must be allowed to wake");
            assert_eq!(policy.repeat, RepeatRule::Always);
        }
    }

    #[test]
    fn steerer_is_the_only_source_with_a_wake_interval() {
        assert_eq!(
            policy_for(&steerer()).min_wake_interval,
            Some(STEERER_WAKE_INTERVAL)
        );
        assert!(policy_for(&mailbox()).min_wake_interval.is_none());
    }

    #[test]
    fn every_source_kind_is_distinct_and_stable() {
        let kinds: Vec<&'static str> = [
            steerer(),
            mailbox(),
            ReminderSource::Lesson,
            ReminderSource::Recall,
            ReminderSource::Diagnostic,
            ReminderSource::SelfIdentity,
            ReminderSource::WorkspaceBriefing,
        ]
        .iter()
        .map(|source| policy_for(source).source_kind)
        .collect();
        assert_eq!(
            kinds,
            vec![
                "steerer",
                "mailbox",
                "lesson",
                "recall",
                "diagnostic",
                "self_identity",
                "workspace_briefing"
            ]
        );
    }
}
