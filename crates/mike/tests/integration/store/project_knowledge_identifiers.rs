use super::knowledge_search_fixture::{Fixture, query, references};
use mike::store::persistence::project_knowledge::KnowledgeQuery;
use serde_json::Value;

const TOPIC_PROMPTS: [&str; 4] = [
    "How do GitLab schedules fire",
    "GitLab schedules run on cron",
    "Pause GitLab schedules overnight",
    "Delete stale GitLab schedules",
];

fn any_term(text: &str) -> KnowledgeQuery {
    KnowledgeQuery {
        any_term: true,
        ..query(text)
    }
}

fn hits(item: &Value) -> i64 {
    item["identifier_hits"].as_i64().unwrap_or(-1)
}

fn find<'a>(items: &'a [Value], reference: &str) -> anyhow::Result<&'a Value> {
    items
        .iter()
        .find(|item| item["reference"] == reference)
        .ok_or_else(|| anyhow::anyhow!("{reference} missing from {items:?}"))
}

async fn topic_turns(fx: &Fixture) -> anyhow::Result<()> {
    for prompt in TOPIC_PROMPTS {
        let session = fx.session().await?;
        fx.turn(&session, prompt, "Answered").await?;
    }
    Ok(())
}

async fn one_turn(fx: &Fixture, prompt: &str) -> anyhow::Result<String> {
    let session = fx.session().await?;
    Ok(format!("turn:{}", fx.turn(&session, prompt, "Done").await?))
}

#[tokio::test]
async fn search_a_turn_holding_the_ticket_id_comes_first_with_one_identifier_hit()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    topic_turns(&fx).await?;
    let ticket = one_turn(&fx, "Look at ABC-1234 when you can").await?;

    let items = fx.search(any_term("ABC-1234 GitLab schedules")).await?;

    assert_eq!(items.len(), 5, "{items:?}");
    assert_eq!(items[0]["reference"], ticket.as_str());
    assert_eq!(hits(&items[0]), 1);
    assert!(
        items[0]["excerpt"]
            .as_str()
            .is_some_and(|text| text.contains("ABC-1234"))
    );
    assert!(items[1..].iter().all(|item| hits(item) == 0), "{items:?}");
    fx.finish().await
}

#[tokio::test]
async fn search_a_turn_holding_only_the_ticket_id_is_a_candidate_without_any_term()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    topic_turns(&fx).await?;
    let ticket = one_turn(&fx, "Look at ABC-1234 when you can").await?;

    let items = fx.search(query("ABC-1234 GitLab schedules")).await?;

    assert_eq!(references(&items), vec![ticket]);
    assert_eq!(hits(&items[0]), 1);
    fx.finish().await
}

#[tokio::test]
async fn search_without_an_identifier_reports_zero_hits_and_keeps_the_rank_order()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    topic_turns(&fx).await?;
    one_turn(&fx, "Look at ABC-1234 when you can").await?;

    let items = fx.search(query("GitLab schedules")).await?;

    assert_eq!(items.len(), 4, "{items:?}");
    assert!(items.iter().all(|item| hits(item) == 0), "{items:?}");
    let ranks: Vec<f64> = items.iter().filter_map(|i| i["rank"].as_f64()).collect();
    assert!(ranks.windows(2).all(|pair| pair[0] >= pair[1]), "{ranks:?}");
    fx.finish().await
}

#[tokio::test]
async fn search_ticket_abc_12_does_not_hit_a_row_holding_only_abc_123() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let longer = one_turn(&fx, "Review ABC-123 and the lemur plan").await?;

    let items = fx.search(any_term("ABC-12 lemur")).await?;

    assert_eq!(hits(find(&items, &longer)?), 0, "{items:?}");
    fx.finish().await
}

#[tokio::test]
async fn search_number_12_does_not_hit_a_row_holding_only_number_123() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let longer = one_turn(&fx, "See #123 about the lemur plan").await?;
    let exact = one_turn(&fx, "See #12 about the lemur plan").await?;

    let items = fx.search(any_term("#12 lemur")).await?;

    assert_eq!(hits(find(&items, &longer)?), 0, "{items:?}");
    assert_eq!(hits(find(&items, &exact)?), 1, "{items:?}");
    assert_eq!(items[0]["reference"], exact.as_str());
    fx.finish().await
}

#[tokio::test]
async fn search_a_short_hash_hits_a_row_holding_the_full_hash() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let full = one_turn(
        &fx,
        "Commit cb7e5268a1b2c3d4e5f60718293a4b5c6d7e8f90 shipped the okapi fix",
    )
    .await?;

    let items = fx.search(query("cb7e526 okapi")).await?;

    assert_eq!(references(&items), vec![full]);
    assert_eq!(hits(&items[0]), 1);
    fx.finish().await
}

#[tokio::test]
async fn search_a_code_name_hits_a_migration_file_name_containing_it() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let file = one_turn(&fx, "Migration 20261109185000_search_grades.sql applied").await?;
    let other = one_turn(&fx, "Migration notes without the table name").await?;

    let items = fx.search(any_term("search_grades migration")).await?;

    assert_eq!(hits(find(&items, &file)?), 1, "{items:?}");
    assert_eq!(hits(find(&items, &other)?), 0, "{items:?}");
    assert_eq!(items[0]["reference"], file.as_str());
    fx.finish().await
}

#[tokio::test]
async fn search_excerpt_is_centred_on_the_identifier_when_the_row_holds_one() -> anyhow::Result<()>
{
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let filler = "padding ".repeat(60);
    let long = one_turn(&fx, &format!("filler {filler} then ZZ-9 appears")).await?;

    let items = fx.search(any_term("filler ZZ-9")).await?;

    let excerpt = find(&items, &long)?["excerpt"].as_str().unwrap_or_default();
    assert!(excerpt.contains("ZZ-9"), "{excerpt}");
    fx.finish().await
}
