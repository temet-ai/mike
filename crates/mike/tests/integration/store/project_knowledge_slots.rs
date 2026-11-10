use super::knowledge_search_fixture::{Fixture, others, query, references};
use mike::store::persistence::project_knowledge::KnowledgeQuery;
use serde_json::Value;

fn of_kind<'a>(items: &'a [Value], kind: &str) -> Vec<&'a Value> {
    items
        .iter()
        .filter(|item| item["source_kind"] == kind)
        .collect()
}

#[tokio::test]
async fn search_one_prompt_in_three_sessions_comes_back_as_one_item_naming_the_other_two()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let mut turns = Vec::new();
    for (prompt, answer) in [
        ("Tidy the marmoset cache", "First session answer"),
        ("  tidy THE marmoset cache  ", "Second session answer"),
        ("Tidy the marmoset cache", "Third session answer"),
    ] {
        let session = fx.session().await?;
        turns.push(format!("turn:{}", fx.turn(&session, prompt, answer).await?));
    }

    let items = fx.search(query("marmoset")).await?;

    let found = of_kind(&items, "turn");
    assert_eq!(found.len(), 1, "{items:?}");
    let item = found[0];
    assert_eq!(item["also_count"], 2);
    let mut seen = others(item);
    seen.push(item["reference"].as_str().unwrap_or_default().to_owned());
    seen.sort();
    turns.sort();
    assert_eq!(seen, turns);
    fx.finish().await
}

#[tokio::test]
async fn search_prefers_the_copy_with_an_answer_when_one_prompt_was_sent_twice()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let (first, second) = (fx.session().await?, fx.session().await?);
    let answered = fx
        .turn(&first, "Prune the gannet branches", "Pruned them")
        .await?;
    fx.turn(&second, "Prune the gannet branches", "").await?;

    let items = fx.search(query("gannet")).await?;

    assert_eq!(references(&items), vec![format!("turn:{answered}")]);
    assert_eq!(items[0]["also_count"], 1);
    fx.finish().await
}

#[tokio::test]
async fn search_a_turn_with_its_correction_and_unmet_ask_comes_back_as_one_item()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = fx.session().await?;
    let turn = fx
        .turn(&session, "Export the capybara report", "Exported")
        .await?;
    let verdict = fx
        .verdict(
            &session,
            turn,
            (
                "Export the capybara report as csv",
                "The capybara export ignored the format",
            ),
        )
        .await?;

    let items = fx.search(query("capybara")).await?;

    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["also_count"], 2);
    let mut all = others(&items[0]);
    all.push(
        items[0]["reference"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    );
    all.sort();
    let mut expected = vec![
        format!("turn:{turn}"),
        format!("correction:{verdict}"),
        format!("unmet_ask:{verdict}"),
    ];
    expected.sort();
    assert_eq!(all, expected);
    fx.finish().await
}

#[tokio::test]
async fn search_with_kind_correction_still_returns_the_correction() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = fx.session().await?;
    let turn = fx
        .turn(&session, "Export the ibis report", "Exported")
        .await?;
    let verdict = fx
        .verdict(
            &session,
            turn,
            (
                "Export the ibis report as csv",
                "The ibis export ignored csv",
            ),
        )
        .await?;

    let items = fx
        .search(KnowledgeQuery {
            kind: Some("correction".into()),
            ..query("ibis")
        })
        .await?;

    assert_eq!(references(&items), vec![format!("correction:{verdict}")]);
    assert_eq!(items[0]["also_count"], 0);
    assert!(others(&items[0]).is_empty());
    fx.finish().await
}

#[tokio::test]
async fn search_two_notes_with_identical_bodies_come_back_as_one_item() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = fx.session().await?;
    let body = "The ocelot queue needs a second consumer";
    let first = fx
        .note(&session, "finding", ("Ocelot finding one", body))
        .await?;
    let second = fx
        .note(&session, "finding", ("Ocelot finding two", body))
        .await?;
    let distinct = fx
        .note(
            &session,
            "finding",
            ("Ocelot other", "The ocelot queue is fine"),
        )
        .await?;

    let items = fx.search(query("ocelot")).await?;

    assert_eq!(items.len(), 2, "{items:?}");
    let pair = items
        .iter()
        .find(|item| item["also_count"] == 1)
        .ok_or_else(|| anyhow::anyhow!("no folded item in {items:?}"))?;
    let mut folded = others(pair);
    folded.push(pair["reference"].as_str().unwrap_or_default().to_owned());
    folded.sort();
    let mut expected = vec![format!("note:{first}"), format!("note:{second}")];
    expected.sort();
    assert_eq!(folded, expected);
    assert!(references(&items).contains(&format!("note:{distinct}")));
    fx.finish().await
}

#[tokio::test]
async fn search_an_interrupted_resend_is_left_out_unless_include_historical_is_set()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = fx.session().await?;
    let interrupted = fx.turn(&session, "Rebuild the okapi index", "").await?;
    let resent = fx
        .turn(
            &session,
            "Rebuild the okapi index and then verify it",
            "Rebuilt and verified",
        )
        .await?;
    let other = fx.session().await?;
    let unanswered = fx
        .turn(&other, "Okapi lonely prompt nobody resent", "")
        .await?;

    let default = fx.search(query("okapi")).await?;
    let historical = fx
        .search(KnowledgeQuery {
            include_historical: true,
            ..query("okapi")
        })
        .await?;

    let mut kept = references(&default);
    kept.sort();
    let mut expected = vec![format!("turn:{resent}"), format!("turn:{unanswered}")];
    expected.sort();
    assert_eq!(kept, expected);
    assert!(references(&historical).contains(&format!("turn:{interrupted}")));
    assert_eq!(historical.len(), 3);
    fx.finish().await
}
