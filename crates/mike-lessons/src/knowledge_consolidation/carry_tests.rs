use super::test_support::{harness, split};

const PARA_A: &str = "Always run the complete test suite before you push any change to the shared main branch of this repository.\n";
const PARA_B: &str = "Never rewrite published history on the shared branches because other sessions depend on the commits there.\n";
const PARA_C: &str = "When you add a database migration, also add the matching schema documentation entry in the same change set.\n";

#[tokio::test]
async fn a_new_revision_keeps_its_statements_and_splits_only_the_new_text() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let v1 = format!("# Rules\n\n{PARA_A}\n{PARA_B}");
    let file = h.fx.checkout_file(project, "rules", "git.md", &v1, h.root.path()).await?;
    h.llm.push_split(0, Ok(split(&[("Run tests before pushing.", PARA_A.trim()), ("Never rewrite published history.", PARA_B.trim())])));
    h.items().await?;
    let before: Vec<i64> = h.fx.statements_of(file.carrier.item_id).await?.iter().map(|r| r.0).collect();
    assert_eq!(before.len(), 2);

    let v2 = format!("{v1}\n{PARA_C}");
    let next = h.fx.revise_file(&file, "git.md", &v2).await?;
    h.llm.push_split(0, Ok(split(&[("Add schema docs with migrations.", PARA_C.trim())])));
    let tally = h.items().await?;
    assert_eq!(tally.statements_kept, 2);
    assert_eq!(tally.statements_created, 1);
    let rows = h.fx.statements_of(next.item_id).await?;
    assert_eq!(rows.len(), 3);
    assert!(before.iter().all(|id| rows.iter().any(|r| r.0 == *id && r.2 == "current")));
    let bodies = h.llm.split_bodies.lock().map(|b| b.clone()).unwrap_or_default();
    assert_eq!(bodies.len(), 2);
    assert!(bodies[1].contains("database migration") && !bodies[1].contains("test suite"));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_file_that_lost_a_line_comes_back_with_no_new_statements_and_no_model_call() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let v1 = format!("# Rules\n\n{PARA_A}\n{PARA_B}");
    let file = h.fx.checkout_file(project, "rules", "git.md", &v1, h.root.path()).await?;
    h.llm.push_split(0, Ok(split(&[("Run tests before pushing.", PARA_A.trim()), ("Never rewrite published history.", PARA_B.trim())])));
    h.items().await?;
    let v2 = format!("# Rules\n\n{PARA_A}");
    let next = h.fx.revise_file(&file, "git.md", &v2).await?;
    let tally = h.items().await?;
    assert_eq!((tally.statements_created, tally.statements_kept, tally.statements_gone), (0, 1, 1));
    assert_eq!(h.llm.split_calls.lock().map(|c| c.len()).unwrap_or(0), 1);
    let rows = h.fx.statements_of(next.item_id).await?;
    assert_eq!(rows.iter().filter(|r| r.2 == "current").count(), 1);
    h.fx.handle_requeue(&next).await?;
    let again = h.items().await?;
    assert_eq!(again.statements_created, 0);
    assert_eq!(h.llm.split_calls.lock().map(|c| c.len()).unwrap_or(0), 1);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_long_file_is_split_in_sections_and_offsets_are_relative_to_the_whole_body() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let filler = "A line of ordinary prose that fills out the section nicely.\n".repeat(420);
    let body = format!("# One\nFirst rule here.\n{filler}# Two\nSecond rule here.\n{filler}# Three\nThird rule here.\n{filler}");
    assert!(body.chars().count() > 50_000);
    h.fx.checkout_file(project, "rules", "long.md", &body, h.root.path()).await?;
    h.llm.push_split(0, Ok(split(&[("First.", "First rule here.")])));
    h.llm.push_split(0, Ok(split(&[("Second.", "Second rule here.")])));
    h.llm.push_split(0, Ok(split(&[("Third.", "Third rule here.")])));
    let tally = h.items().await?;
    assert_eq!(tally.statements_created, 3);
    assert_eq!(h.llm.split_calls.lock().map(|c| c.len()).unwrap_or(0), 3);
    let rows: Vec<(String, i32, i32)> = sqlx::query_as(
        "SELECT text, passage_start, passage_end FROM memory.knowledge_statements ORDER BY ordinal",
    )
    .fetch_all(h.fx.handle.pool())
    .await?;
    let found: Vec<&str> = rows.iter().map(|(_, s, e)| &body[*s as usize..*e as usize]).collect();
    assert_eq!(found, vec!["First rule here.", "Second rule here.", "Third rule here."]);
    h.fx.db.cleanup().await?;
    Ok(())
}
