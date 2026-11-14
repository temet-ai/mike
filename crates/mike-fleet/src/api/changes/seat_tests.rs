use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::path::Path;
use std::process::Command;

use serde_json::json;
use uuid::Uuid;

use super::resolve::{FleetSeat, Resolved};
use super::tail::lines_for;
use super::{WithTail, standing_view, whose};
use crate::change_record::answers;
use crate::change_record::{ChangeKind, SessionIdentity};
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::definition::TaskDefinition;
use crate::launch::fleet::seated::Seated;
use crate::store::persistence::change_record::test_seed::{self, CHECKOUT};

fn seated_at(session: Uuid) -> Seated {
    Seated::default().worker("T4", session, "worker-T4")
}

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(args)
        .status()?;
    anyhow::ensure!(status.success(), "git {args:?} failed in {dir:?}");
    Ok(())
}

fn clone_one_commit_ahead() -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    let remote = root.path().join("remote.git");
    let work = root.path().join("work");
    std::fs::create_dir(&remote)?;
    git(&remote, &["init", "--bare", "-b", "main"])?;
    std::fs::create_dir(&work)?;
    git(&work, &["init", "-b", "main"])?;
    git(
        &work,
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().unwrap_or_default(),
        ],
    )?;
    std::fs::write(work.join("a.txt"), "one")?;
    git(&work, &["add", "."])?;
    git(&work, &["commit", "-m", "first"])?;
    git(&work, &["push", "-u", "origin", "main"])?;
    std::fs::write(work.join("a.txt"), "two")?;
    git(&work, &["commit", "-am", "second"])?;
    Ok(root)
}

fn definition(repo: &str, session: Uuid) -> FleetDefinition {
    let mut def = FleetDefinition {
        repo: repo.to_owned(),
        branch: "main".to_owned(),
        ..FleetDefinition::default()
    };
    def.tasks.insert(
        "T4".to_owned(),
        TaskDefinition {
            name: "worker-T4".to_owned(),
            files: vec!["src/mine".to_owned()],
            seat: Some("worker-T4".to_owned()),
            session: Some(session.to_string()),
            state: Some("working".to_owned()),
            ..TaskDefinition::default()
        },
    );
    def
}

async fn identity(f: &test_seed::Fixture, id: i64) -> anyhow::Result<SessionIdentity> {
    f.handle
        .session_identities(&[id])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("session {id} has no identity"))
}

#[tokio::test]
async fn standing_for_a_fleet_seat_reports_task_state_open_paths_ahead_and_the_check()
-> anyhow::Result<()> {
    let Some(f) = test_seed::fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project = test_seed::project(pool).await?;
    let me = test_seed::session(pool, project, None).await?;
    let me_identity = identity(&f, me).await?;
    let repo = clone_one_commit_ahead()?;
    let work = repo.path().join("work");
    let work_text = work.to_string_lossy().into_owned();
    let mut edit = test_seed::event(project, ChangeKind::Edit, &["src/mine/x.rs"]);
    edit.checkout = work_text.clone();
    edit.session_id = Some(me);
    f.handle.fleet().insert_change_events(&[edit]).await?;
    let check = f
        .handle
        .fleet()
        .begin_tree_check("run-1", "state-1", "scripts/cx check", "worker-T4")
        .await?;
    f.handle
        .fleet()
        .finish_tree_check(check, Some(0), "ok", Some("state-1"))
        .await?;
    let def = definition(&work_text, me_identity.uuid);
    let resolved = Resolved {
        identity: Some(me_identity),
        checkout: work_text,
        fleet: Some(FleetSeat {
            definition: def,
            run: "run-1".to_owned(),
            task: "T4".to_owned(),
            seat: "worker-T4".to_owned(),
            files: vec!["src/mine".to_owned()],
        }),
    };

    let value = standing_view::standing(&f.handle, &resolved).await?;
    assert_eq!(value["uncommitted"], json!(["src/mine/x.rs"]), "{value}");
    assert_eq!(value["fleet"]["task"], "T4", "{value}");
    assert_eq!(value["fleet"]["state"], "working", "{value}");
    assert_eq!(value["fleet"]["ahead"], 1, "{value}");
    assert_eq!(value["fleet"]["behind"], 0, "{value}");
    assert_eq!(value["fleet"]["latest_check"]["passed"], true, "{value}");
    assert!(value.get("others").is_none(), "{value}");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn whose_names_the_owning_task_beside_the_last_changer() -> anyhow::Result<()> {
    let Some(f) = test_seed::fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project = test_seed::project(pool).await?;
    let other = test_seed::session(pool, project, None).await?;
    let mut edit = test_seed::event(project, ChangeKind::Edit, &["src/mine/x.rs"]);
    edit.session_id = Some(other);
    f.handle.fleet().insert_change_events(&[edit]).await?;
    let me = Uuid::new_v4();
    let resolved = Resolved {
        identity: None,
        checkout: CHECKOUT.to_owned(),
        fleet: Some(FleetSeat {
            definition: definition(CHECKOUT, me),
            run: "run-1".to_owned(),
            task: "T4".to_owned(),
            seat: "worker-T4".to_owned(),
            files: vec!["src/mine".to_owned()],
        }),
    };
    let value = whose(
        &f.handle,
        &resolved,
        Some(&format!("{CHECKOUT}/src/mine/x.rs")),
    )
    .await?;
    assert_eq!(value["task"], "T4", "{value}");
    assert_eq!(value["seat"], "worker-T4", "{value}");
    assert_eq!(value["last_change"]["how"], "edited", "{value}");
    assert_eq!(value["committed"], false, "{value}");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_tail_is_none_when_nothing_changed_and_at_most_twelve_lines_after_changes()
-> anyhow::Result<()> {
    let Some(f) = test_seed::fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project = test_seed::project(pool).await?;
    let me = test_seed::session(pool, project, None).await?;
    let other = test_seed::session(pool, project, None).await?;
    let me_identity = identity(&f, me).await?;
    let def = definition(CHECKOUT, me_identity.uuid);

    let mut first = test_seed::event(project, ChangeKind::Edit, &["src/mine/first.rs"]);
    first.session_id = Some(other);
    f.handle.fleet().insert_change_events(&[first]).await?;
    assert_eq!(
        lines_for(
            &f.handle,
            &def,
            &seated_at(me_identity.uuid),
            me_identity.uuid
        )
        .await,
        None
    );
    assert_eq!(
        lines_for(
            &f.handle,
            &def,
            &seated_at(me_identity.uuid),
            me_identity.uuid
        )
        .await,
        None
    );

    let many: Vec<_> = (0..30)
        .map(|n| {
            let mut e =
                test_seed::event(project, ChangeKind::Edit, &[&format!("src/mine/f{n}.rs")]);
            e.session_id = Some(other);
            e
        })
        .collect();
    f.handle.fleet().insert_change_events(&many).await?;
    let lines = lines_for(
        &f.handle,
        &def,
        &seated_at(me_identity.uuid),
        me_identity.uuid,
    )
    .await
    .ok_or_else(|| anyhow::anyhow!("expected lines after 30 changes"))?;
    assert!(!lines.is_empty() && lines.len() <= 12, "{lines:?}");

    let report = json!({"seat": "worker-T4"});
    let with = serde_json::to_value(WithTail {
        report,
        since_you_last_looked: Some(lines.clone()),
    })?;
    assert_eq!(with["seat"], "worker-T4");
    assert_eq!(with["since_you_last_looked"], json!(lines));
    let without = serde_json::to_value(WithTail {
        report: json!({"seat": "worker-T4"}),
        since_you_last_looked: None,
    })?;
    assert_eq!(without, json!({"seat": "worker-T4"}));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_proxy_tail_and_the_api_tail_share_one_place_so_lines_show_once() -> anyhow::Result<()>
{
    let Some(f) = test_seed::fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project = test_seed::project(pool).await?;
    let me = test_seed::session(pool, project, None).await?;
    let other = test_seed::session(pool, project, None).await?;
    let me_identity = identity(&f, me).await?;
    let def = definition(CHECKOUT, me_identity.uuid);
    let resolved = Resolved {
        identity: Some(me_identity.clone()),
        checkout: CHECKOUT.to_owned(),
        fleet: Some(FleetSeat {
            definition: def.clone(),
            run: "run-1".to_owned(),
            task: "T4".to_owned(),
            seat: "worker-T4".to_owned(),
            files: vec!["src/mine".to_owned()],
        }),
    };
    let caller = resolved.caller()?;
    let mut baseline = test_seed::event(project, ChangeKind::Edit, &["src/mine/base.rs"]);
    baseline.session_id = Some(other);
    f.handle.fleet().insert_change_events(&[baseline]).await?;
    assert!(answers::tail(&f.handle, &caller).await?.is_empty());

    let mut edit = test_seed::event(project, ChangeKind::Edit, &["src/mine/a.rs"]);
    edit.session_id = Some(other);
    f.handle.fleet().insert_change_events(&[edit]).await?;
    let from_api = answers::tail(&f.handle, &caller).await?;
    assert!(!from_api.is_empty(), "the mod's read shows the new line");
    assert_eq!(
        lines_for(
            &f.handle,
            &def,
            &seated_at(me_identity.uuid),
            me_identity.uuid
        )
        .await,
        None
    );
    assert!(answers::tail(&f.handle, &caller).await?.is_empty());

    let mut again = test_seed::event(project, ChangeKind::Edit, &["src/mine/b.rs"]);
    again.session_id = Some(other);
    f.handle.fleet().insert_change_events(&[again]).await?;
    let from_proxy = lines_for(
        &f.handle,
        &def,
        &seated_at(me_identity.uuid),
        me_identity.uuid,
    )
    .await;
    assert!(from_proxy.is_some(), "the proxy's read shows the next line");
    assert!(answers::tail(&f.handle, &caller).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}
