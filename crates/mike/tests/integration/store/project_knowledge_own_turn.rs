use super::knowledge_search_fixture::{Fixture, Session, others, query, references};
use mike::store::persistence::project_knowledge::KnowledgeQuery;

struct Seeded {
    fx: Fixture,
    session: String,
    owner: Session,
    older: i64,
    newest: i64,
    verdict: i64,
}

async fn seeded() -> anyhow::Result<Option<Seeded>> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(None);
    };
    let session = fx.session().await?;
    let older = fx
        .turn(&session, "Plan the lemur rollout", "Planned")
        .await?;
    let newest = fx
        .turn(&session, "Where does the lemur rollout stand", "")
        .await?;
    let verdict = fx
        .verdict(
            &session,
            newest,
            (
                "Report the lemur rollout state",
                "The lemur question went unanswered",
            ),
        )
        .await?;
    Ok(Some(Seeded {
        session: session.uuid.to_string(),
        owner: session,
        fx,
        older,
        newest,
        verdict,
    }))
}

fn mentions_own_turn(items: &[serde_json::Value], seeded: &Seeded) -> bool {
    let own = [
        format!("turn:{}", seeded.newest),
        format!("correction:{}", seeded.verdict),
        format!("unmet_ask:{}", seeded.verdict),
    ];
    items.iter().any(|item| {
        let mut found = others(item);
        found.push(item["reference"].as_str().unwrap_or_default().to_owned());
        found.iter().any(|reference| own.contains(reference))
    })
}

#[tokio::test]
async fn search_with_a_caller_session_leaves_out_its_newest_turn_and_that_turns_corrections()
-> anyhow::Result<()> {
    let Some(seeded) = seeded().await? else {
        return Ok(());
    };

    let items = seeded
        .fx
        .search(KnowledgeQuery {
            caller_session: Some(seeded.session.clone()),
            ..query("lemur")
        })
        .await?;

    assert_eq!(references(&items), vec![format!("turn:{}", seeded.older)]);
    assert!(!mentions_own_turn(&items, &seeded));
    seeded.fx.finish().await
}

#[tokio::test]
async fn search_with_a_machine_opened_turn_after_the_prompt_still_leaves_out_the_operators_turn()
-> anyhow::Result<()> {
    let Some(seeded) = seeded().await? else {
        return Ok(());
    };
    let wake = seeded.fx.turn(&seeded.owner, "", "").await?;
    sqlx::query("UPDATE memory.operator_turns SET opened_by = 'machine' WHERE id = $1")
        .bind(wake)
        .execute(seeded.fx.handle.pool())
        .await?;

    let items = seeded
        .fx
        .search(KnowledgeQuery {
            caller_session: Some(seeded.session.clone()),
            ..query("lemur")
        })
        .await?;

    assert_eq!(references(&items), vec![format!("turn:{}", seeded.older)]);
    assert!(!mentions_own_turn(&items, &seeded));
    seeded.fx.finish().await
}

#[tokio::test]
async fn search_with_include_own_turn_returns_the_callers_newest_turn() -> anyhow::Result<()> {
    let Some(seeded) = seeded().await? else {
        return Ok(());
    };

    let items = seeded
        .fx
        .search(KnowledgeQuery {
            caller_session: Some(seeded.session.clone()),
            include_own_turn: true,
            ..query("lemur")
        })
        .await?;

    assert!(mentions_own_turn(&items, &seeded), "{items:?}");
    seeded.fx.finish().await
}

#[tokio::test]
async fn search_without_a_caller_session_returns_every_turn() -> anyhow::Result<()> {
    let Some(seeded) = seeded().await? else {
        return Ok(());
    };

    let items = seeded.fx.search(query("lemur")).await?;

    assert!(mentions_own_turn(&items, &seeded), "{items:?}");
    assert!(references(&items).contains(&format!("turn:{}", seeded.older)));
    seeded.fx.finish().await
}

#[tokio::test]
async fn search_with_a_caller_session_that_is_not_a_uuid_returns_every_turn() -> anyhow::Result<()>
{
    let Some(seeded) = seeded().await? else {
        return Ok(());
    };

    let items = seeded
        .fx
        .search(KnowledgeQuery {
            caller_session: Some("not-a-session".into()),
            ..query("lemur")
        })
        .await?;

    assert!(mentions_own_turn(&items, &seeded), "{items:?}");
    seeded.fx.finish().await
}
