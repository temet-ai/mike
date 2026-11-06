use super::*;
use std::path::Path;

async fn git(repo: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

async fn commit(repo: &Path, content: &str) -> anyhow::Result<String> {
    std::fs::write(repo.join("ids.txt"), content)?;
    git(repo, &["add", "ids.txt"]).await?;
    git(repo, &["commit", "-qm", "ids"]).await?;
    git(repo, &["rev-parse", "HEAD"]).await
}

async fn repository(h: &Harness, repo: &Path) -> anyhow::Result<(Uuid, String)> {
    git(repo, &["init", "-q", "-b", "main"]).await?;
    git(repo, &["config", "user.email", "test@example.invalid"]).await?;
    git(repo, &["config", "user.name", "test"]).await?;
    let sha = commit(repo, "stable\n").await?;
    let fleet = h
        .persist
        .find_fleet("report")
        .await?
        .ok_or_else(|| anyhow::anyhow!("fleet missing"))?;
    let mut definition = seated();
    definition["repo"] = json!(repo.to_string_lossy());
    definition["branch"] = json!("main");
    definition["check"] = json!({"command": "true"});
    h.persist
        .save_fleet_definition(fleet.fleet_uuid, &definition, &json!([]), fleet.updated_at)
        .await?
        .ok_or_else(|| anyhow::anyhow!("definition not saved"))?;
    h.committed(&sha).await?;
    Ok((fleet.fleet_uuid, sha))
}

async fn rule(h: &Harness, fleet: Uuid, data: &Value) -> anyhow::Result<Uuid> {
    let tags = vec!["acceptance-obligation".into(), format!("fleet:{fleet}")];
    let note = NewSessionNote {
        kind: Some("task"),
        title: "Q29",
        body: None,
        status: Some("open"),
        tags: Some(&tags),
        data: Some(data),
    };
    assert!(
        h.persist
            .create_session_note(id(WORKER), note)
            .await
            .is_err()
    );
    Ok(h.persist
        .create_session_note(id(ACCEPTANCE), note)
        .await?
        .note_uuid)
}

async fn report_outcome(h: &Harness, sha: &str, outcome: &str) -> anyhow::Result<Value> {
    let (code, report) = h.post("report", report_body(WORKER, sha)).await?;
    assert_eq!(code, StatusCode::OK, "{report}");
    assert_eq!(
        report["proof"]["rulings"][0]["outcome"], outcome,
        "{report}"
    );
    Ok(report)
}

async fn check(h: &Harness, note: Uuid, sha: &str) -> anyhow::Result<Value> {
    let (code, result) = h
        .post(
            "check",
            json!({
                "session": id(WORKER), "ruling": note, "commit": sha, "wait_seconds": 5
            }),
        )
        .await?;
    assert_eq!(code, StatusCode::OK, "{result}");
    Ok(result)
}

fn private_state_dir() -> anyhow::Result<tempfile::TempDir> {
    let state = tempfile::tempdir()?;
    let mike = state.path().join("mike");
    std::fs::create_dir(&mike)?;
    std::fs::set_permissions(&mike, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    unsafe { std::env::set_var("XDG_STATE_HOME", state.path()) };
    Ok(state)
}

#[tokio::test]
async fn registered_ruling_executes_and_report_requires_its_exact_commit_and_version()
-> anyhow::Result<()> {
    let state = private_state_dir()?;
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    let (fleet, sha) = repository(&h, root.path()).await?;
    let mut data = json!({
        "obligation_id": "Q29", "tasks": ["P1"], "criteria": ["C1"], "outcome": "pending",
        "ruling": {"version": 1, "command": "grep -q '^stable$' ids.txt", "expected_exit_code": 0}
    });
    let note = rule(&h, fleet, &data).await?;
    report_outcome(&h, &sha, "missing").await?;
    let passed = check(&h, note, &sha).await?;
    assert_eq!(passed["passed"], true, "{passed}");
    let first = report_outcome(&h, &sha, "passed").await?;
    assert_eq!(first["proof"]["rulings"][0]["commit"], sha);
    assert!(first["proof"]["rulings"][0]["check_id"].is_i64());
    let broken = commit(root.path(), "derived\n").await?;
    h.committed(&broken).await?;
    report_outcome(&h, &broken, "missing").await?;
    assert_eq!(check(&h, note, &broken).await?["passed"], false);
    report_outcome(&h, &broken, "failed").await?;
    assert_eq!(check(&h, note, &sha).await?["passed"], true);
    report_outcome(&h, &broken, "failed").await?;
    data["supersedes"] = json!(note);
    data["ruling"]["version"] = json!(2);
    let next = rule(&h, fleet, &data).await?;
    let fresh = report_outcome(&h, &broken, "missing").await?;
    assert_eq!(fresh["proof"]["rulings"][0]["note"], next.to_string());
    drop(state);
    h.cleanup().await
}

#[tokio::test]
async fn ruling_contract_is_immutable_and_other_session_cannot_register_through_update()
-> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    let (fleet, _) = repository(&h, root.path()).await?;
    let mut data = json!({
        "obligation_id": "Q29", "tasks": ["P1"], "outcome": "pending",
        "ruling": {"version": 1, "command": "true", "expected_exit_code": 0}
    });
    let note = rule(&h, fleet, &data).await?;
    let base = h
        .base
        .split("/api/")
        .next()
        .ok_or_else(|| anyhow::anyhow!("API base"))?;
    data["ruling"]["command"] = json!("false");
    let url = |session| format!("{base}/api/sessions/{}/notes/{note}", id(session));
    assert_eq!(
        h.client
            .patch(url(WORKER))
            .json(&json!({"data": data}))
            .send()
            .await?
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        h.client
            .patch(url(ACCEPTANCE))
            .json(&json!({"data": data}))
            .send()
            .await?
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        h.client.delete(url(ACCEPTANCE)).send().await?.status(),
        StatusCode::BAD_REQUEST
    );
    h.cleanup().await
}
