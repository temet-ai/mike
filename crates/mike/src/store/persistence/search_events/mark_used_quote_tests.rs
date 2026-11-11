use serde_json::{Value, json};

use super::fixture_tests::{Fixture, SEARCH_TOOL};
use super::mark_used::USED_BATCH;

const PASSAGE: &str = "The reranker passages exceed budget because candidates arrive unsorted \
                       during warmup, so the first request waits for the whole batch.";
const EIGHT_WORDS: &str = "reranker passages exceed budget because candidates arrive unsorted";
const SEVEN_WORDS: &str = "reranker passages exceed budget because candidates arrive";

async fn shown_search(fixture: &Fixture, entries: Value) -> anyhow::Result<i64> {
    fixture
        .tool_use(
            1,
            SEARCH_TOOL,
            json!({"project": "p", "query": "slow reranker"}),
        )
        .await?;
    fixture
        .record_returned("search_project_knowledge", "slow reranker", &entries)
        .await
}

async fn mark(fixture: &Fixture) -> anyhow::Result<()> {
    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;
    Ok(())
}

fn first_shown() -> Value {
    json!([
        {"reference": "lesson:a", "score": 0.5, "shown": PASSAGE},
        {"reference": "lesson:b", "score": 0.4}
    ])
}

#[tokio::test]
async fn mark_used_when_the_final_answer_repeats_eight_words_of_the_shown_passage_marks_it()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = shown_search(&fixture, first_shown()).await?;
    fixture
        .final_reply(2, &format!("The cause: {EIGHT_WORDS} during startup."))
        .await?;

    mark(&fixture).await?;

    assert_eq!(fixture.used(event).await?.0, vec!["lesson:a".to_owned()]);
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_final_answer_repeats_only_seven_words_marks_nothing()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = shown_search(&fixture, first_shown()).await?;
    fixture
        .final_reply(2, &format!("The cause: {SEVEN_WORDS} and then more."))
        .await?;

    mark(&fixture).await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_run_is_also_in_the_turns_prompt_marks_nothing() -> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = shown_search(&fixture, first_shown()).await?;
    fixture
        .user_prompt(2, &format!("Is it true that {EIGHT_WORDS}?"))
        .await?;
    fixture
        .final_reply(2, &format!("Yes, {EIGHT_WORDS} during startup."))
        .await?;

    mark(&fixture).await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_final_answer_names_the_reference_marks_it_with_shown_text_present()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = shown_search(&fixture, first_shown()).await?;
    fixture
        .final_reply(2, "Following lesson:b, the warmup is the cause.")
        .await?;

    mark(&fixture).await?;

    assert_eq!(fixture.used(event).await?.0, vec!["lesson:b".to_owned()]);
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_a_row_has_no_shown_text_a_quote_marks_nothing_and_a_named_reference_still_does()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = shown_search(
        &fixture,
        json!([{"reference": "lesson:a", "score": 0.5}, {"reference": "lesson:b", "score": 0.4}]),
    )
    .await?;
    fixture
        .final_reply(2, &format!("{EIGHT_WORDS} during startup, per lesson:b."))
        .await?;

    mark(&fixture).await?;

    assert_eq!(fixture.used(event).await?.0, vec!["lesson:b".to_owned()]);
    fixture.cleanup().await
}
