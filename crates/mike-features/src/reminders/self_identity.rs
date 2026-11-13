use crate::proxy::remote_control::inject::PushDeps;
use crate::store::models::ReminderSource;
use crate::store::models::RequestId;
use crate::store::models::{SELF_IDENTITY_ID_LINE, SELF_IDENTITY_MARKER};

use super::{RemindersOutcome, push_or_append_reminder};

pub struct SelfIdentityRequest<'a> {
    pub body_bytes: &'a [u8],
    pub session_id: &'a str,
    pub request_id: RequestId,
    pub push_deps: &'a PushDeps,
}

pub async fn apply_self_identity(req: SelfIdentityRequest<'_>) -> RemindersOutcome {
    let SelfIdentityRequest {
        body_bytes,
        session_id,
        request_id,
        push_deps,
    } = req;

    push_or_append_reminder(super::PushOrAppendRequest {
        push_deps,
        session_id,
        body_bytes,
        source: ReminderSource::SelfIdentity,
        reminder_text: if crate::explained::first_time(session_id, crate::explained::SELF_IDENTITY)
        {
            build_self_identity_reminder_text(session_id)
        } else {
            self_identity_id_line(session_id)
        },
        source_finding_ids: Vec::new(),
        source_request_id: 0,
        request_id,
    })
    .await
}

pub fn self_identity_id_line(session_id: &str) -> String {
    format!(
        "<system-reminder source=\"{SELF_IDENTITY_MARKER}\">\n{SELF_IDENTITY_ID_LINE}{session_id}\n</system-reminder>"
    )
}

pub fn build_self_identity_reminder_text(session_id: &str) -> String {
    format!(
        "<system-reminder source=\"{SELF_IDENTITY_MARKER}\">\n\
{SELF_IDENTITY_ID_LINE}{session_id}\n\
When the user refers to \"this session\" or asks what the session id is, this is the value.\n\
\n\
Pass this exact id as `session_id` to the mike workspace tools that act on your own behalf:\n\
workspace_heartbeat (renew your lease and read your inbox), send_session_message (pass\n\
`request_reply: true` to have it filled in for you, or pass `from` with this exact id),\n\
register_workspace_session, set_session_goal, ack_session_message and\n\
end_workspace_registration. Never guess or reuse a peer's id for those calls.\n\
\n\
This id is the conversation id, not necessarily a harness process id: an id you see inside a\n\
filesystem path this process wrote (for example a /tmp/claude-* directory, a task-output file\n\
path, or a scratchpad directory) can belong to the harness process rather than to this\n\
conversation, and the two are not always the same value. For mailbox addressing (`session_id`,\n\
`from`, or a peer lookup), always use the id named at the top of this reminder, never one\n\
scraped from a filesystem path.\n\
\n\
Artifact-frontmatter convention: when you create or contribute to an artifact .md document\n\
(a design doc, ADR, strategy doc, spec, or worklog — a deliverable, not throwaway scratch),\n\
record this session_id in the document's YAML frontmatter as a block-style session_id list.\n\
Do NOT add frontmatter to conventional repo files that render raw at host roots — README,\n\
CHANGELOG, LICENSE, CONTRIBUTING — leave those plain. Append this id only if it is not already\n\
present, and never overwrite or remove existing ids — multiple sessions contribute to the\n\
same document over time.\n\
</system-reminder>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reminders::test_support::noop_push_deps;
    use crate::store::models::RequestId;

    fn user_turn_body() -> Vec<u8> {
        serde_json::json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]}
            ]
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn reminder_text_names_the_session_id_and_the_workspace_tools() {
        let text = build_self_identity_reminder_text("abc-123");
        assert!(text.starts_with("<system-reminder source=\"proxy-self-identity\">"));
        assert!(text.ends_with("</system-reminder>"));
        assert!(text.contains("session_id is: abc-123"));
        assert!(text.contains("workspace_heartbeat"));
        assert!(text.contains("send_session_message"));
        assert!(text.contains("register_workspace_session"));
        assert!(text.contains("set_session_goal"));
        assert!(text.contains("ack_session_message"));
        assert!(text.contains("end_workspace_registration"));
        assert!(text.contains("request_reply"));
        assert!(text.contains("/tmp/claude-*"));
        assert!(text.contains("harness process"));
    }

    #[tokio::test]
    async fn apply_self_identity_appends_to_a_user_turn() -> anyhow::Result<()> {
        let body = user_turn_body();
        let outcome = apply_self_identity(SelfIdentityRequest {
            body_bytes: &body,
            session_id: "test-session-id",
            request_id: RequestId::next(),
            push_deps: &noop_push_deps(),
        })
        .await;
        let RemindersOutcome::Appended {
            body: new_body,
            reminder_text,
            source_finding_ids,
            source_request_id,
        } = outcome
        else {
            anyhow::bail!("expected Appended for a well-formed user-turn body");
        };
        assert!(reminder_text.contains("test-session-id"));
        assert!(source_finding_ids.is_empty());
        assert_eq!(source_request_id, 0);
        let new_body_str = String::from_utf8(new_body)?;
        assert!(new_body_str.contains("test-session-id"));
        Ok(())
    }

    #[tokio::test]
    async fn the_explanation_goes_once_and_later_turns_carry_only_the_id_line() {
        let body = user_turn_body();
        let text = || async {
            match apply_self_identity(SelfIdentityRequest {
                body_bytes: &body,
                session_id: "identity-once",
                request_id: RequestId::next(),
                push_deps: &noop_push_deps(),
            })
            .await
            {
                RemindersOutcome::Appended { reminder_text, .. } => reminder_text,
                _ => String::new(),
            }
        };
        assert!(text().await.contains("workspace_heartbeat"));
        assert_eq!(
            text().await,
            "<system-reminder source=\"proxy-self-identity\">\nThis Claude Code session's session_id is: identity-once\n</system-reminder>"
        );
    }

    #[tokio::test]
    async fn apply_self_identity_unchanged_for_a_non_object_body() {
        let outcome = apply_self_identity(SelfIdentityRequest {
            body_bytes: b"not json",
            session_id: "test-session-id",
            request_id: RequestId::next(),
            push_deps: &noop_push_deps(),
        })
        .await;
        assert!(matches!(outcome, RemindersOutcome::Unchanged));
    }

    #[tokio::test]
    async fn apply_self_identity_reasserts_when_the_marker_is_absent_from_a_fresh_body()
    -> anyhow::Result<()> {
        let body = user_turn_body();
        let push_deps = noop_push_deps();
        let first = apply_self_identity(SelfIdentityRequest {
            body_bytes: &body,
            session_id: "repeat-session-id",
            request_id: RequestId::next(),
            push_deps: &push_deps,
        })
        .await;
        assert!(matches!(first, RemindersOutcome::Appended { .. }));

        let second = apply_self_identity(SelfIdentityRequest {
            body_bytes: &body,
            session_id: "repeat-session-id",
            request_id: RequestId::next(),
            push_deps: &push_deps,
        })
        .await;
        assert!(
            matches!(second, RemindersOutcome::Appended { .. }),
            "a body that never carried the identity marker (compaction dropped it, or it \
             simply never arrived) must have it asserted again, even for a session that \
             already received it once"
        );
        Ok(())
    }

    #[tokio::test]
    async fn apply_self_identity_suppresses_a_second_delivery_while_the_marker_is_still_present()
    -> anyhow::Result<()> {
        let body = user_turn_body();
        let push_deps = noop_push_deps();
        let first = apply_self_identity(SelfIdentityRequest {
            body_bytes: &body,
            session_id: "repeat-session-id",
            request_id: RequestId::next(),
            push_deps: &push_deps,
        })
        .await;
        let RemindersOutcome::Appended {
            body: injected_body,
            ..
        } = first
        else {
            anyhow::bail!("expected Appended for a well-formed user-turn body");
        };

        let second = apply_self_identity(SelfIdentityRequest {
            body_bytes: &injected_body,
            session_id: "repeat-session-id",
            request_id: RequestId::next(),
            push_deps: &push_deps,
        })
        .await;
        let RemindersOutcome::Suppressed { reason } = second else {
            anyhow::bail!(
                "expected the second delivery to be suppressed while the marker is still \
                 present in this request's own history"
            );
        };
        assert_eq!(reason, crate::injection::ALREADY_IN_REQUEST);
        Ok(())
    }
}
