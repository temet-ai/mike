use super::knowledge_search_fixture::{Fixture, query, references};
use serde_json::Value;
use uuid::Uuid;

fn written_by(item: &Value) -> Option<&str> {
    item["provenance"]["written_by"].as_str()
}

fn ranked_lower(item: &Value) -> bool {
    item["provenance"]["ranked_lower"].as_bool() == Some(true)
}

fn position(items: &[Value], reference: &str) -> anyhow::Result<usize> {
    references(items)
        .iter()
        .position(|found| found == reference)
        .ok_or_else(|| anyhow::anyhow!("{reference} is missing from {items:?}"))
}

async fn mail_thread(fx: &Fixture, session_id: i64, subject: &str) -> anyhow::Result<String> {
    let uuid: Uuid = sqlx::query_scalar(
        "INSERT INTO mailbox.threads (thread_uuid, subject, project_id, created_by_session_id)
         VALUES (gen_random_uuid(), $1, $2, $3) RETURNING thread_uuid",
    )
    .bind(subject)
    .bind(fx.project)
    .bind(session_id)
    .fetch_one(fx.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, from_session_id, thread_id, body)
         SELECT gen_random_uuid(), $1, t.id, $3 FROM mailbox.threads t WHERE t.thread_uuid = $2",
    )
    .bind(session_id)
    .bind(uuid)
    .bind(format!("{subject} reply"))
    .execute(fx.handle.pool())
    .await?;
    Ok(format!("thread:{uuid}"))
}

#[tokio::test]
async fn search_marks_machine_written_rows_and_ranks_the_lowered_ones_below_an_operator_turn()
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
    let thread = mail_thread(&fx, writer.id, "quokka quokka quokka thread").await?;

    let items = fx.search(query("quokka")).await?;

    let operator_at = position(&items, &format!("turn:{operator}"))?;
    assert!(
        !references(&items).contains(&format!("turn:{tool_turn}")),
        "a tool-written turn is a machine turn and is not searched as the operator's"
    );
    for reference in [format!("note:{brief}"), thread.clone()] {
        let at = position(&items, &reference)?;
        assert_eq!(written_by(&items[at]), Some("machine"), "{reference}");
        assert!(ranked_lower(&items[at]), "{reference}");
        assert!(
            at > operator_at,
            "{reference} ranks above the operator turn"
        );
    }
    let mail_at = position(&items, &format!("mail:{mail}"))?;
    assert_eq!(written_by(&items[mail_at]), Some("machine"));
    assert!(!ranked_lower(&items[mail_at]));
    assert_eq!(written_by(&items[operator_at]), None);
    assert!(!ranked_lower(&items[operator_at]));
    let finding_at = position(&items, &format!("note:{finding}"))?;
    assert_eq!(written_by(&items[finding_at]), None);
    fx.finish().await
}

#[tokio::test]
async fn search_does_not_halve_the_rank_of_a_mail_row_that_matches_like_an_operator_turn()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let (writer, reader) = (fx.session().await?, fx.session().await?);
    let operator = fx
        .turn(&writer, "Compare the quokka plans", "Compared")
        .await?;
    let mail = fx
        .mail(&writer, &reader, "Compare the quokka plans")
        .await?;

    let items = fx.search(query("quokka")).await?;

    let operator_item = &items[position(&items, &format!("turn:{operator}"))?];
    let mail_item = &items[position(&items, &format!("mail:{mail}"))?];
    assert_eq!(operator_item["lexical_rank"], mail_item["lexical_rank"]);
    let operator_rank = operator_item["rank"].as_f64().unwrap_or(0.0);
    let mail_rank = mail_item["rank"].as_f64().unwrap_or(0.0);
    assert!(
        mail_rank > operator_rank * 0.9,
        "mail {mail_rank} operator {operator_rank}"
    );
    fx.finish().await
}

#[tokio::test]
async fn search_leaves_out_every_tool_written_opening() -> anyhow::Result<()> {
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

    let found = references(&items);
    for reference in &machine {
        assert!(!found.contains(reference), "{reference} is a machine turn");
    }
    let operator_at = position(&items, &format!("turn:{operator}"))?;
    assert_eq!(written_by(&items[operator_at]), None);
    fx.finish().await
}
