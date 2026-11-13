use mike::store::persistence::cross_session::ApproveDirectiveProposal;
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use mike::store::persistence::steerer_repo::SteererPersist as _;

use super::{fixture, lesson_stage_and_source, open_directive_proposal, proposal_status};

#[tokio::test]
async fn approving_a_directive_proposal_writes_a_daily_lesson_and_links_it() -> anyhow::Result<()> {
    let Some(f) = fixture("improvement-approve").await? else {
        return Ok(());
    };
    let proposal_id = open_directive_proposal(f.handle.pool(), f.project_id).await?;

    let params = ApproveDirectiveProposal {
        id: proposal_id,
        text: "commit only after the tests are green",
        kind: "correction",
        target_project_ids: &[f.project_id],
        expires_at: None,
    };
    let lessons = f
        .handle
        .steerer()
        .approve_directive_proposal(&params)
        .await?;
    assert_eq!(lessons.len(), 1);
    assert_eq!(lessons[0].project_id, f.project_id);

    let (stage, source) = lesson_stage_and_source(f.handle.pool(), lessons[0].lesson_id).await?;
    assert_eq!(stage, "daily");
    assert_eq!(source, "directive");

    let linked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.directive_lessons WHERE proposal_id = $1 AND lesson_id = $2",
    )
    .bind(proposal_id)
    .bind(lessons[0].lesson_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(linked, 1);

    assert_eq!(
        proposal_status(f.handle.pool(), proposal_id).await?,
        "approved"
    );

    let injectable = f
        .handle
        .lessons()
        .list_injectable_lessons(&f.project_path)
        .await?;
    assert!(
        injectable
            .iter()
            .any(|l| l.id == lessons[0].lesson_id && l.stage == "daily"),
        "the approved lesson must be injectable for its project"
    );
    Ok(())
}

#[tokio::test]
async fn approving_an_already_decided_proposal_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture("improvement-approve-twice").await? else {
        return Ok(());
    };
    let proposal_id = open_directive_proposal(f.handle.pool(), f.project_id).await?;
    let params = ApproveDirectiveProposal {
        id: proposal_id,
        text: "commit only after the tests are green",
        kind: "correction",
        target_project_ids: &[f.project_id],
        expires_at: None,
    };
    let first = f
        .handle
        .steerer()
        .approve_directive_proposal(&params)
        .await?;
    assert_eq!(first.len(), 1);

    let second = f
        .handle
        .steerer()
        .approve_directive_proposal(&params)
        .await?;
    assert!(
        second.is_empty(),
        "a decided proposal cannot be approved again"
    );
    Ok(())
}

#[tokio::test]
async fn approving_into_every_target_project_writes_one_lesson_each() -> anyhow::Result<()> {
    let Some(f) = fixture("improvement-approve-fanout").await? else {
        return Ok(());
    };
    let second_project =
        mike::store::test_support::seed_project(f.handle.pool(), "improvement-approve-fanout-2")
            .await?;
    let proposal_id = open_directive_proposal(f.handle.pool(), f.project_id).await?;

    let params = ApproveDirectiveProposal {
        id: proposal_id,
        text: "commit only after the tests are green",
        kind: "correction",
        target_project_ids: &[f.project_id, second_project],
        expires_at: None,
    };
    let lessons = f
        .handle
        .steerer()
        .approve_directive_proposal(&params)
        .await?;
    assert_eq!(lessons.len(), 2);
    let mut project_ids: Vec<i64> = lessons.iter().map(|l| l.project_id).collect();
    project_ids.sort_unstable();
    let mut expected = [f.project_id, second_project];
    expected.sort_unstable();
    assert_eq!(project_ids, expected);
    Ok(())
}

#[tokio::test]
async fn approving_into_a_missing_project_fails_and_leaves_the_proposal_open() -> anyhow::Result<()>
{
    let Some(f) = fixture("improvement-approve-bad-project").await? else {
        return Ok(());
    };
    let proposal_id = open_directive_proposal(f.handle.pool(), f.project_id).await?;
    let params = ApproveDirectiveProposal {
        id: proposal_id,
        text: "commit only after the tests are green",
        kind: "correction",
        target_project_ids: &[999_999_999],
        expires_at: None,
    };
    let result = f.handle.steerer().approve_directive_proposal(&params).await;
    assert!(
        result.is_err(),
        "a nonexistent project must fail the whole write"
    );
    assert_eq!(proposal_status(f.handle.pool(), proposal_id).await?, "open");
    Ok(())
}
