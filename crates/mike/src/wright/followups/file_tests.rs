use super::body::marker;
use super::fakes_tests::{CHECK, Env, FakeRunner, FakeSessions, REPO, ok};
use super::file::{Filing, file};
use super::runner::Program;
use crate::store::persistence::wright::FollowupLimitsPatch;

fn sessions() -> FakeSessions {
    FakeSessions { ended: None }
}

#[tokio::test]
async fn file_existing_issue_with_marker_is_adopted_not_filed_again() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env.limits(FollowupLimitsPatch::default()).await?;
    let (id, _) = env.followup("hardening", "launch").await?;
    let listed = serde_json::json!([
        {"number": 41, "url": format!("https://github.com/{REPO}/issues/41"), "body": "other <!-- wright-followup:999999 -->"},
        {"number": 42, "url": format!("https://github.com/{REPO}/issues/42"), "body": format!("old text\n{}", marker(id))},
    ])
    .to_string();
    let runner = FakeRunner::new().on(Program::Gh, "issue list", ok(&listed));
    let ctx = env.ctx(runner, sessions());
    let report = file(&ctx, &limits).await?;
    assert_eq!(report.filed, vec![(id, Filing::Adopted(42))]);
    assert!(ctx.runner.called(Program::Gh, "issue create").is_empty());
    let search = ctx.runner.called(Program::Gh, "issue list");
    assert!(search[0].has_args(&["--label", "wright", "--state", "all"]));
    assert!(search[0].has_args(&[&format!("--search=wright-followup:{id} in:body")]));
    assert_eq!(env.state_of(id).await?, ("filed".to_owned(), 0, Some(42)));
    env.db.cleanup().await
}

#[tokio::test]
async fn file_daily_cap_reached_files_no_more_that_day() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env
        .limits(FollowupLimitsPatch {
            daily_filed: Some(1),
            ..FollowupLimitsPatch::default()
        })
        .await?;
    let (first, _) = env.followup("hardening", "launch").await?;
    let (second, _) = env.followup("hardening", "mailbox").await?;
    let runner = FakeRunner::new()
        .on(Program::Gh, "issue list", ok("[]"))
        .on(
            Program::Gh,
            "issue create",
            ok(&format!(
                "Creating issue\nhttps://github.com/{REPO}/issues/77\n"
            )),
        );
    let ctx = env.ctx(runner, sessions());
    let report = file(&ctx, &limits).await?;
    assert_eq!(report.filed, vec![(first, Filing::Created(77))]);
    let again = file(&ctx, &limits).await?;
    assert!(again.capped);
    assert!(again.filed.is_empty());
    assert_eq!(ctx.runner.called(Program::Gh, "issue create").len(), 1);
    assert_eq!(env.state_of(second).await?.0, "open");
    env.db.cleanup().await
}

#[tokio::test]
async fn file_new_issue_body_carries_marker_and_evidence_links() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env.limits(FollowupLimitsPatch::default()).await?;
    let (id, problem) = env.followup("hardening", "launch").await?;
    let runner = FakeRunner::new()
        .on(Program::Gh, "issue list", ok("[]"))
        .on(
            Program::Gh,
            "issue create",
            ok(&format!("https://github.com/{REPO}/issues/88\n")),
        );
    let ctx = env.ctx(runner, sessions());
    file(&ctx, &limits).await?;
    let created = ctx.runner.called(Program::Gh, "issue create");
    let [create] = created.as_slice() else {
        anyhow::bail!("expected one issue create, got {created:?}");
    };
    assert!(create.has_args(&["--title=Launches are left open"]));
    assert!(create.has_args(&["--body-file", "-"]));
    let body = create.stdin.clone().unwrap_or_default();
    assert_eq!(body.lines().last(), Some(marker(id).as_str()));
    assert!(body.starts_with("The launch ledger keeps rows"));
    assert!(body.contains(&format!("http://localhost:9990/wright#problem-{problem}")));
    assert!(body.contains(&format!("- Check: `{CHECK}`")));
    assert!(body.contains("- Owning part: `launch`"));
    assert!(body.contains("- Kind: `hardening`"));
    assert_eq!(env.state_of(id).await?, ("filed".to_owned(), 0, Some(88)));
    env.db.cleanup().await
}
