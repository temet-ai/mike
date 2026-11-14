use anyhow::Context;
use mike::store::persistence::prompt_classification::ClassificationWrite;

use super::{fixture, plain_request, stored_classification};

fn write_with_key<'a>(
    session_db_id: i64,
    request_id: i64,
    kind_code: &'a str,
    rule: Option<&'a str>,
    repeat_key: Option<&'a str>,
) -> ClassificationWrite<'a> {
    ClassificationWrite {
        request_id,
        session_db_id,
        kind_code,
        rule,
        repeat_key,
        classifier_version: "genuine-v3",
    }
}

#[tokio::test]
async fn apply_prompt_origin_operator_answer_clears_the_repeat_key_and_keeps_the_kind()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9201, 0, "keep going with the plan until it is finished").await?;
    f.handle
        .write_classification_row(&write_with_key(
            f.session_id,
            9201,
            "operator",
            None,
            Some("keep going"),
        ))
        .await?;
    let rows = f.handle.apply_prompt_origin(9201, "operator", true).await?;
    assert_eq!(rows, 1);
    let stored = stored_classification(f.handle.pool(), 9201)
        .await?
        .context("classification row must exist")?;
    assert_eq!(stored.0, "operator");
    assert_eq!(stored.2, None);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn apply_prompt_origin_machine_answer_moves_the_kind_and_keeps_the_repeat_key()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9202, 0, "check the build and report back").await?;
    f.handle
        .write_classification_row(&write_with_key(
            f.session_id,
            9202,
            "operator",
            None,
            Some("check the build"),
        ))
        .await?;
    let rows = f
        .handle
        .apply_prompt_origin(9202, "loop-wakeup", false)
        .await?;
    assert_eq!(rows, 1);
    let stored = stored_classification(f.handle.pool(), 9202)
        .await?
        .context("classification row must exist")?;
    assert_eq!(stored.0, "loop-wakeup");
    assert_eq!(stored.2.as_deref(), Some("check the build"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn apply_prompt_origin_leaves_a_marker_matched_row_alone() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9203, 0, "Tool loaded.").await?;
    f.handle
        .write_classification_row(&write_with_key(
            f.session_id,
            9203,
            "tool-loaded",
            Some("a"),
            None,
        ))
        .await?;
    let rows = f.handle.apply_prompt_origin(9203, "operator", true).await?;
    assert_eq!(rows, 0);
    let stored = stored_classification(f.handle.pool(), 9203)
        .await?
        .context("classification row must exist")?;
    assert_eq!(stored.0, "tool-loaded");
    f.db.cleanup().await?;
    Ok(())
}
