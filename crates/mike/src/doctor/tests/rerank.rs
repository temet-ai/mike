use super::*;

#[test]
fn rerank_check_passes_when_the_reranker_is_not_enabled() -> anyhow::Result<()> {
    let checks = evaluate(&healthy());
    let rerank = check(&checks, "rerank")?;
    assert_eq!(rerank.status, Status::Ok);
    assert!(rerank.fix.is_none());
    Ok(())
}

#[test]
fn rerank_check_passes_when_the_enabled_reranker_answers() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.rerank = RerankFacts {
        url: Some("http://omit:11435".to_string()),
        reachable: Some(true),
        detail: None,
    };
    let checks = evaluate(&observed);
    let rerank = check(&checks, "rerank")?;
    assert_eq!(rerank.status, Status::Ok);
    assert!(rerank.detail.contains("http://omit:11435"));
    Ok(())
}

#[test]
fn rerank_check_warns_with_the_setup_step_when_the_server_does_not_answer() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.rerank = RerankFacts {
        url: Some("http://omit:11435".to_string()),
        reachable: Some(false),
        detail: Some("Reranker at http://omit:11435 did not answer: refused".to_string()),
    };
    let checks = evaluate(&observed);
    let rerank = check(&checks, "rerank")?;
    assert_eq!(rerank.status, Status::Warn);
    assert!(
        rerank.detail.contains("did not answer"),
        "{}",
        rerank.detail
    );
    assert_eq!(rerank.fix.as_deref(), Some("mike setup rerank"));
    assert_eq!(rerank.setup_step, Some(SetupStep::Rerank));
    Ok(())
}

#[test]
fn rerank_check_names_the_setup_step_when_the_server_row_carries_no_detail() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.rerank = RerankFacts {
        url: Some("http://omit:11435".to_string()),
        reachable: Some(false),
        detail: None,
    };
    let checks = evaluate(&observed);
    let rerank = check(&checks, "rerank")?;
    assert_eq!(rerank.status, Status::Warn);
    assert!(
        rerank.detail.contains("does not answer"),
        "{}",
        rerank.detail
    );
    assert_eq!(rerank.fix.as_deref(), Some("mike setup rerank"));
    Ok(())
}

#[test]
fn rerank_check_passes_unprobed_when_mike_is_not_running() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.rerank = RerankFacts {
        url: Some("http://omit:11435".to_string()),
        ..RerankFacts::default()
    };
    let checks = evaluate(&observed);
    assert_eq!(check(&checks, "rerank")?.status, Status::Ok);
    Ok(())
}
