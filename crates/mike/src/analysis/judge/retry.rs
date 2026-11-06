use std::future::Future;
use std::time::Duration;

use super::outcome::{
    JudgeOutcome, log_and_backoff, log_and_credential_failure, log_and_fail, log_and_refuse,
};
use super::{MAX_TRANSIENT_ATTEMPTS, is_transient};
use crate::llm::LlmError;

pub(crate) async fn with_judge_retry<T, F, Fut>(
    turn_id: i64,
    backoff_base: Duration,
    mut call: F,
) -> Result<T, JudgeOutcome>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, LlmError>>,
{
    let mut attempt = 1;
    loop {
        match call().await {
            Ok(value) => return Ok(value),
            Err(e) if e.is_refusal() => return Err(log_and_refuse(turn_id, attempt, &e)),
            Err(e) if e.is_credential_failure() => {
                return Err(log_and_credential_failure(turn_id, attempt, &e));
            }
            Err(e) if is_transient(&e) && attempt < MAX_TRANSIENT_ATTEMPTS => {
                log_and_backoff(turn_id, attempt, &e, backoff_base).await;
                attempt += 1;
            }
            Err(e) => return Err(log_and_fail(turn_id, attempt, &e)),
        }
    }
}
