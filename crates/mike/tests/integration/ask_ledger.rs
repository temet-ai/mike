use chrono::Duration;
use mike::dossier::entries::link_ledger_step;
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::memory_ask_ledger::NewAskLedgerEntry;

use crate::turn_facts::resolution::message;
use crate::turn_facts::{Fixture, Req, fixture, insert_request};

fn keyword_entry(f: &Fixture, request_id: i64, content: &str) -> NewAskLedgerEntryOwned {
    NewAskLedgerEntryOwned {
        session_id: f.session_id,
        request_id,
        content: content.to_owned(),
    }
}

struct NewAskLedgerEntryOwned {
    session_id: i64,
    request_id: i64,
    content: String,
}

impl NewAskLedgerEntryOwned {
    fn as_entry(&self) -> NewAskLedgerEntry<'_> {
        NewAskLedgerEntry {
            session_id: self.session_id,
            request_id: Some(self.request_id),
            operator_turn_id: None,
            kind: "initial",
            kind_source: "keyword",
            content: &self.content,
        }
    }
}

async fn session_uuid(f: &Fixture) -> anyhow::Result<uuid::Uuid> {
    Ok(
        sqlx::query_scalar("SELECT session_uuid FROM sessions.sessions WHERE id = $1")
            .bind(f.session_id)
            .fetch_one(f.handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn widened_append_stores_request_id_and_a_repeat_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "build the importer")).await?;
    let owned = keyword_entry(&f, 9100, "build the importer");
    let first = f
        .handle
        .dossier()
        .append_ask_ledger_entry(&owned.as_entry())
        .await?;
    assert!(first.is_some());
    let repeat = f
        .handle
        .dossier()
        .append_ask_ledger_entry(&owned.as_entry())
        .await?;
    assert!(repeat.is_none(), "a repeat request_id writes nothing");
    let rows = f
        .handle
        .dossier()
        .fetch_ask_ledger_inspection(session_uuid(&f).await?, 10)
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].request_id, Some(9100));
    assert_eq!(rows[0].kind_source, "keyword");
    assert_eq!(rows[0].keyword_kind.as_deref(), Some("initial"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn model_fetch_drops_a_machine_classified_row_that_inspection_keeps() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "build the importer")).await?;
    insert_request(&f, &Req::primary(9101, 10, "agent relay text")).await?;
    sqlx::query(
        "INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id)
         SELECT 9101, $1, k.id FROM prompt_kinds.kinds k WHERE k.code = 'agent-lineage'",
    )
    .bind(f.session_id)
    .execute(f.handle.pool())
    .await?;
    for (id, text) in [(9100, "build the importer"), (9101, "agent relay text")] {
        let owned = keyword_entry(&f, id, text);
        f.handle
            .dossier()
            .append_ask_ledger_entry(&owned.as_entry())
            .await?;
    }
    let uuid = session_uuid(&f).await?;
    let for_model = f
        .handle
        .dossier()
        .fetch_ask_ledger_for_model(uuid, 10)
        .await?;
    let contents: Vec<&str> = for_model.iter().map(|r| r.content.as_str()).collect();
    assert_eq!(contents, vec!["build the importer"]);
    let inspection = f
        .handle
        .dossier()
        .fetch_ask_ledger_inspection(uuid, 10)
        .await?;
    assert_eq!(inspection.len(), 2);
    let machine = inspection
        .iter()
        .find(|r| r.request_id == Some(9101))
        .ok_or_else(|| anyhow::anyhow!("the machine row must stay visible"))?;
    assert!(!machine.genuine);
    assert_eq!(
        f.handle.dossier().fetch_ask_ledger(uuid, 10).await?.len(),
        2
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn builder_path_adds_one_linked_row_for_an_operator_turn_and_none_for_a_machine_turn()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let operator = message(&f, 9100, 0, "please add a retry to the client", Some(100)).await?;
    let machine = message(&f, 9101, 100, "Tool loaded.", Some(200)).await?;
    sqlx::query(
        "UPDATE memory.operator_turns
            SET opened_by = CASE WHEN id = $1 THEN 'operator' ELSE 'machine' END
          WHERE id = ANY($2)",
    )
    .bind(operator)
    .bind(vec![operator, machine])
    .execute(f.handle.pool())
    .await?;
    let since = Some(f.base - Duration::days(1));
    let report = link_ledger_step(&f.handle, &[f.session_id], since).await?;
    assert_eq!(report.ledger_rows_inserted, 1, "report: {report:?}");
    let again = link_ledger_step(&f.handle, &[f.session_id], since).await?;
    assert_eq!(again.ledger_rows_inserted, 0);
    let rows = f
        .handle
        .dossier()
        .fetch_ask_ledger_inspection(session_uuid(&f).await?, 10)
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].operator_turn_id, Some(operator));
    assert_eq!(rows[0].request_id, Some(9100));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn ledger_builder_since_reads_jc03_migration_install_time() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert!(f.handle.dossier().ledger_builder_since().await?.is_some());
    f.db.cleanup().await?;
    Ok(())
}
