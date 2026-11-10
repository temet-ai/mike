use super::knowledge_search_fixture::{Fixture, query, references};
use serde_json::Value;

fn written_by(item: &Value) -> Option<&str> {
    item["provenance"]["written_by"].as_str()
}

fn position(items: &[Value], reference: &str) -> anyhow::Result<usize> {
    references(items)
        .iter()
        .position(|found| found == reference)
        .ok_or_else(|| anyhow::anyhow!("{reference} is missing from {items:?}"))
}

#[tokio::test]
async fn search_marks_machine_written_rows_and_ranks_them_below_an_operator_turn()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let (writer, reader) = (fx.session().await?, fx.session().await?);
    let operator = fx
        .turn(&writer, "Compare the quokka plans", "Compared")
        .await?;
    let tool_turn = fx
        .turn(
            &reader,
            "Perform a web search for the query: quokka quokka quokka",
            "Searched",
        )
        .await?;
    let brief = fx
        .note(
            &writer,
            "brief",
            ("Launch brief: plans", "quokka quokka quokka briefing"),
        )
        .await?;
    let finding = fx
        .note(
            &writer,
            "finding",
            ("Plan finding", "quokka quokka quokka finding"),
        )
        .await?;
    let mail = fx
        .mail(&writer, &reader, "quokka quokka quokka mail from a seat")
        .await?;

    let items = fx.search(query("quokka")).await?;

    let operator_at = position(&items, &format!("turn:{operator}"))?;
    for reference in [
        format!("turn:{tool_turn}"),
        format!("note:{brief}"),
        format!("mail:{mail}"),
    ] {
        let at = position(&items, &reference)?;
        assert_eq!(written_by(&items[at]), Some("machine"), "{reference}");
        assert!(
            at > operator_at,
            "{reference} ranks above the operator turn"
        );
    }
    assert_eq!(written_by(&items[operator_at]), None);
    let finding_at = position(&items, &format!("note:{finding}"))?;
    assert_eq!(written_by(&items[finding_at]), None);
    fx.finish().await
}

#[tokio::test]
async fn search_marks_every_tool_written_opening_as_machine_written() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = fx.session().await?;
    let mut machine = Vec::new();
    for opening in [
        "A session-scoped Stop hook is now active with condition:",
        "The tether plugin sent a message:",
        "Perform a web search for the query:",
        "You score how well a stand-in predicted",
        "Coordinator wake for fleet ",
    ] {
        let prompt = format!("{opening} {} narwhal", machine.len());
        machine.push(format!("turn:{}", fx.turn(&session, &prompt, "ok").await?));
    }
    let operator = fx
        .turn(&session, "Please check the narwhal numbers", "checked")
        .await?;

    let items = fx.search(query("narwhal")).await?;

    for reference in &machine {
        let at = position(&items, reference)?;
        assert_eq!(written_by(&items[at]), Some("machine"), "{reference}");
    }
    let operator_at = position(&items, &format!("turn:{operator}"))?;
    assert_eq!(written_by(&items[operator_at]), None);
    fx.finish().await
}
