use sha2::{Digest, Sha256};

use crate::store::persistence::SessionForJudgment;
use crate::store::persistence::judge_digest::JudgeAttemptKey;

use super::cycle::SessionJudgeCtx;

pub(super) fn digest_sha256(system_prompt: &str, user_message: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(system_prompt.as_bytes());
    hasher.update([0u8]);
    hasher.update(user_message.as_bytes());
    hasher.finalize().to_vec()
}

fn key<'a>(ctx: &'a SessionJudgeCtx<'_>, session: &SessionForJudgment) -> JudgeAttemptKey<'a> {
    JudgeAttemptKey {
        session_db_id: session.session_db_id,
        judge_model: &ctx.runtime.model,
        rubric_version: ctx.rubric_version,
    }
}

pub(super) async fn is_repeat(
    ctx: &SessionJudgeCtx<'_>,
    session: &SessionForJudgment,
    digest: &[u8],
    at: chrono::DateTime<chrono::Utc>,
) -> bool {
    match ctx
        .persist
        .skip_repeated_digest(&key(ctx, session), digest, at)
        .await
    {
        Ok(true) => {
            log_repeat_skipped(&session.session_id);
            true
        }
        Ok(false) => false,
        Err(e) => {
            log_repeat_check_failed(&session.session_id, &e);
            false
        }
    }
}

pub(super) async fn remember(
    ctx: &SessionJudgeCtx<'_>,
    session: &SessionForJudgment,
    digest: &[u8],
) {
    if let Err(e) = ctx
        .persist
        .record_judged_digest(&key(ctx, session), digest)
        .await
    {
        log_remember_failed(&session.session_id, &e);
    }
}

fn log_repeat_skipped(session_id: &str) {
    tracing::debug!(
        session_id,
        "session_judge: digest matches the last judged one — skipping the LLM call"
    );
}

fn log_repeat_check_failed(session_id: &str, e: &crate::store::persistence::PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: digest-repeat check failed — judging");
}

fn log_remember_failed(session_id: &str, e: &crate::store::persistence::PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: judged digest not recorded");
}

#[cfg(test)]
mod tests {
    use super::digest_sha256;

    #[test]
    fn same_prompt_and_digest_hash_the_same() {
        assert_eq!(digest_sha256("sys", "turns"), digest_sha256("sys", "turns"));
    }

    #[test]
    fn a_changed_digest_or_system_prompt_changes_the_hash() {
        let base = digest_sha256("sys", "turns");
        assert_ne!(base, digest_sha256("sys", "turns plus one"));
        assert_ne!(base, digest_sha256("sys2", "turns"));
    }

    #[test]
    fn the_separator_keeps_prompt_and_digest_boundaries_distinct() {
        assert_ne!(digest_sha256("ab", "c"), digest_sha256("a", "bc"));
    }
}
