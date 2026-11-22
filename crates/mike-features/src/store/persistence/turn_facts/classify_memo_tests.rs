use crate::store::persistence::dossier_repo::DossierPersist as _;

use uuid::Uuid;

use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::prompt_classification::ClassificationWrite;
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_captured_request, seed_project,
};

const VERSION: &str = "classify-memo-test-v1";

struct Machines {
    db: TestDbHandle,
    here: PersistHandle,
    other: PersistHandle,
    feed: ChangeFeed,
    session: Uuid,
    session_id: i64,
    request_id: i64,
}

async fn machines() -> anyhow::Result<Option<Machines>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let other = PersistHandle::connect(&db.database_url).await?;
    let feed = ChangeFeed::new();
    here.attach_change_feed(feed.clone());
    let session = Uuid::new_v4();
    let project = seed_project(other.pool(), &format!("/classify-memo/{session}")).await?;
    let request_id = seed_captured_request(
        other.pool(),
        &session.to_string(),
        project,
        "primary",
        b"{}",
    )
    .await?;
    let session_id = sqlx::query_scalar("SELECT session_id FROM requests.requests WHERE id = $1")
        .bind(request_id)
        .fetch_one(other.pool())
        .await?;
    Ok(Some(Machines {
        db,
        here,
        other,
        feed,
        session,
        session_id,
        request_id,
    }))
}

impl Machines {
    async fn pending_here(&self, version: &str) -> anyhow::Result<Vec<i64>> {
        Ok(self
            .here
            .dossier()
            .load_prompts_needing_classification(&[self.session_id], version)
            .await?
            .into_iter()
            .map(|p| p.request_id)
            .collect())
    }

    async fn classify_on_other_machine(&self) -> anyhow::Result<()> {
        self.other
            .write_classification_row(&ClassificationWrite {
                request_id: self.request_id,
                session_db_id: self.session_id,
                kind_code: "operator",
                rule: None,
                repeat_key: None,
                classifier_version: VERSION,
            })
            .await?;
        Ok(())
    }

    async fn drop_classification_on_other_machine(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM prompt_kinds.classifications WHERE request_id = $1")
            .bind(self.request_id)
            .execute(self.other.pool())
            .await?;
        Ok(())
    }

    fn requests_moved(&self) {
        self.feed
            .record(RowChangedTable::Requests, Some(&self.session.to_string()));
    }
}

#[tokio::test]
async fn classified_session_without_new_requests_is_not_queried_again() -> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    assert_eq!(m.pending_here(VERSION).await?, vec![m.request_id]);
    m.classify_on_other_machine().await?;
    assert!(m.pending_here(VERSION).await?.is_empty());
    m.drop_classification_on_other_machine().await?;
    assert!(
        m.pending_here(VERSION).await?.is_empty(),
        "a session found fully classified is not queried while its requests are unchanged"
    );
    m.requests_moved();
    assert_eq!(m.pending_here(VERSION).await?, vec![m.request_id]);
    m.db.cleanup().await
}

#[tokio::test]
async fn classified_session_is_queried_again_under_another_classifier_version() -> anyhow::Result<()>
{
    let Some(m) = machines().await? else {
        return Ok(());
    };
    m.classify_on_other_machine().await?;
    assert!(m.pending_here(VERSION).await?.is_empty());
    assert_eq!(
        m.pending_here("classify-memo-test-v2").await?,
        vec![m.request_id]
    );
    m.db.cleanup().await
}

#[tokio::test]
async fn session_with_pending_prompts_is_queried_every_time() -> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    assert_eq!(m.pending_here(VERSION).await?, vec![m.request_id]);
    assert_eq!(m.pending_here(VERSION).await?, vec![m.request_id]);
    m.db.cleanup().await
}
