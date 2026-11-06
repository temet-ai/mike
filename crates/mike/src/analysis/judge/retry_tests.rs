use super::*;

#[test]
fn is_transient_true_for_429_5xx_and_timeout() {
    assert!(is_transient(&LlmError::Http { status: 429 }));
    assert!(is_transient(&LlmError::Http { status: 500 }));
    assert!(is_transient(&LlmError::Http { status: 503 }));
    assert!(is_transient(&LlmError::Timeout));
}

#[test]
fn is_transient_false_for_other_4xx_and_non_http_errors() -> anyhow::Result<()> {
    assert!(!is_transient(&LlmError::Http { status: 400 }));
    assert!(!is_transient(&LlmError::Http { status: 401 }));
    assert!(!is_transient(&LlmError::Http { status: 404 }));
    assert!(!is_transient(&LlmError::Transport("boom".to_owned())));

    let Err(schema_err) = serde_json::from_str::<Value>("not json") else {
        anyhow::bail!("expected a parse error for invalid JSON input");
    };
    assert!(!is_transient(&LlmError::Schema(schema_err)));
    Ok(())
}

fn candidate_row(turn_id: i64) -> JudgeCandidateRow {
    JudgeCandidateRow {
        turn_id,
        user_prompt: "fix the bug".to_owned(),
        final_assistant_text: "I fixed the bug.".to_owned(),
        tool_call_count: 1,
        tool_error_count: 0,
    }
}

fn accepted_response() -> Value {
    json!({
        "choices": [{
            "message": {
                "content": r#"{"label":"accepted","confidence":0.9,"rationale":"delivered the fix","unmet_ask":"","evidence_quality":"verified"}"#
            }
        }]
    })
}

fn judge_client_for(server: &wiremock::MockServer) -> anyhow::Result<crate::llm::OpenRouterClient> {
    crate::llm::OpenRouterClient::new(
        "sk-test".to_owned(),
        "test-model".to_owned(),
        Some(server.uri()),
        None,
    )
    .map(|client| client.with_role(crate::llm::LlmRole::JudgeWorker))
    .map_err(Into::into)
}

#[tokio::test]
async fn run_judge_retries_429_then_succeeds() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(accepted_response()))
        .mount(&server)
        .await;

    let llm = judge_client_for(&server)?;
    let verdict = run_judge(&llm, &candidate_row(1), judge_system_prompt()).await;

    assert_eq!(
        verdict.map(|v| v.label),
        Ok(JudgeLabel::Accepted),
        "a 429 followed by a success must yield a recorded verdict, not an error"
    );
    Ok(())
}

#[tokio::test]
async fn run_judge_does_not_retry_permanent_http_error() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(400))
        .expect(1)
        .mount(&server)
        .await;

    let llm = judge_client_for(&server)?;
    let verdict = run_judge(&llm, &candidate_row(2), judge_system_prompt()).await;

    assert_eq!(
        verdict,
        Err(JudgeOutcome::Failed(JudgeFailure::Permanent)),
        "a permanent HTTP error must return Err(JudgeOutcome::Failed(JudgeFailure::Permanent))"
    );
    Ok(())
}

#[tokio::test]
async fn run_judge_exhausts_retry_budget_on_persistent_429() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429))
        .expect(u64::from(MAX_TRANSIENT_ATTEMPTS))
        .mount(&server)
        .await;

    let llm = judge_client_for(&server)?;
    let verdict = run_judge_with_backoff_base(
        &llm,
        &candidate_row(3),
        judge_system_prompt(),
        std::time::Duration::from_millis(5),
    )
    .await;

    assert_eq!(
        verdict,
        Err(JudgeOutcome::Failed(JudgeFailure::TransientExhausted)),
        "a persistent 429 must return Err(JudgeOutcome::Failed(JudgeFailure::TransientExhausted)) \
         after exhausting the retry budget, distinguishable from a permanent failure"
    );
    Ok(())
}

#[tokio::test]
async fn run_judge_returns_refused_on_governor_refusal_without_retrying() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(accepted_response()))
        .expect(0)
        .mount(&server)
        .await;

    crate::llm::limiter::exhaust_global_rate_for_tests();
    let llm = judge_client_for(&server)?;
    let verdict = run_judge(&llm, &candidate_row(4), judge_system_prompt()).await;
    crate::llm::limiter::reset_global_for_tests();

    assert_eq!(
        verdict,
        Err(JudgeOutcome::Refused),
        "a governor refusal must return Err(JudgeOutcome::Refused), not a JudgeFailure variant"
    );
    Ok(())
}

#[tokio::test]
async fn run_judge_returns_credential_failure_on_401_without_retrying() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;

    let llm = judge_client_for(&server)?;
    let verdict = run_judge(&llm, &candidate_row(5), judge_system_prompt()).await;

    assert_eq!(
        verdict,
        Err(JudgeOutcome::CredentialFailure),
        "a 401 must return Err(JudgeOutcome::CredentialFailure), got {verdict:?}"
    );
    Ok(())
}
