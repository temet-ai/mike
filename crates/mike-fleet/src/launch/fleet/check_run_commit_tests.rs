use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::{SEAT_SESSION, inbox, project, queue, queued_check, results, seat_commit};
use crate::launch::fleet::check::Runner;
use crate::launch::fleet::events::{EventPayload, NewFleetEvent, SeatLaunchConfirmed};
use crate::launch::fleet::managed_test_support as managed;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::test_support::acquire_ephemeral_db;

#[tokio::test]
async fn each_new_commit_is_checked_without_an_ask_and_the_committer_gets_the_verdict_for_its_files()
-> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        persist
            .launch()
            .ensure_session_exists(&Uuid::from_u128(SEAT_SESSION).to_string())
            .await?;
        let run = "fleet-queue-proof";
        persist
            .launch()
            .upsert_run(&NewRun {
                run_id: run.to_owned(),
                project_path: Some("/tmp/queue-proof".to_owned()),
                backend: "herdr".to_owned(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await?;
        persist
            .fleet()
            .append_fleet_event(&NewFleetEvent {
                run_id: run.to_owned(),
                task: Some("M".to_owned()),
                session_uuid: Some(Uuid::from_u128(SEAT_SESSION)),
                payload: EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed { launch_id: 1 }),
            })
            .await?;
        let root = tempfile::tempdir()?;
        let (repo, def) = project(root.path()).await?;
        let tracker = TaskTracker::new();
        let fleet = managed::running_fleet(&persist, &def, run).await?;
        let runner = Runner {
            persist: &persist,
            tracker: &tracker,
            run,
            fleet,
        };

        Box::pin(queue(&runner, &def)).await;
        tracker.close();
        tracker.wait().await;
        let passed = persist
            .fleet()
            .latest_tree_check(run)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no check was recorded"))?;
        assert!(passed.passed(), "{}", passed.output);
        assert_eq!(passed.askers, vec!["mike".to_string()]);
        let titles = results(&persist).await?;
        assert_eq!(titles.len(), 1, "{titles:?}");
        assert!(titles[0].ends_with("1 of 1 commit(s) proven"), "{titles:?}");

        let unchanged = queued_check(&runner, &def, &tracker, &persist, run).await?;
        assert_eq!(
            Some(unchanged.id),
            Some(passed.id),
            "a commit already checked is not checked again"
        );

        let bad = seat_commit(&repo, "mine/a.txt", "bad\n").await?;
        let failed = queued_check(&runner, &def, &tracker, &persist, run).await?;
        assert!(!failed.passed());
        assert_eq!(failed.told, vec!["mine".to_string()]);
        let mail = inbox(&persist).await?;
        assert!(
            mail.iter()
                .any(|m| m.contains("fails at commit") && m.contains("mine/a.txt:1")),
            "{mail:?}"
        );
        let titles = results(&persist).await?;
        assert!(
            titles[0].contains(&format!("failed at commit {}", &bad[..10]))
                && titles[0].ends_with("in this task's files"),
            "{titles:?}"
        );

        let fixed = seat_commit(&repo, "mine/a.txt", "ok again\n").await?;
        let final_check = queued_check(&runner, &def, &tracker, &persist, run).await?;
        assert!(final_check.passed(), "{}", final_check.output);
        let titles = results(&persist).await?;
        assert_eq!(titles.len(), 3, "{titles:?}");
        assert!(
            titles[0].contains(&format!("passed at commit {}", &fixed[..10])),
            "{titles:?}"
        );

        let same_state = seat_commit(&repo, "notes.md", "outside the check paths\n").await?;

        let reused = queued_check(&runner, &def, &tracker, &persist, run).await?;
        assert_eq!(Some(reused.id), Some(final_check.id));
        let titles = results(&persist).await?;
        assert_eq!(titles.len(), 4, "{titles:?}");
        assert!(
            titles[0].contains(&format!("passed at commit {}", &same_state[..10])),
            "{titles:?}"
        );
        db.cleanup().await?;
        Ok(())
    })
    .await?
}
