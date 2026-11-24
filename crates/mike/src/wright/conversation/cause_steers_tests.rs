use std::sync::Arc;

use serde_json::{Value, json};

use super::fakes_tests::{Db, FakeEngine};
use super::steer_apply::Steers;
use super::worker::tick;
use super::{Change, SteerApply, SteerError, steering};
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::wright::{CausePatch, CauseRow};

pub(super) const NAMES: [&str; 3] = ["alpha", "bravo", "charlie"];

pub(super) struct Fixture {
    pub(super) db: Db,
    pub(super) steers: Steers,
    pub(super) causes: Vec<i64>,
}

pub(super) struct Applied {
    pub(super) prior: Value,
    pub(super) effect: Value,
}

impl Fixture {
    pub(super) async fn boot() -> anyhow::Result<Self> {
        let db = Db::boot().await?;
        sqlx::query(
            "INSERT INTO wright.checks (name, source, cadence, expects, probe, query)
             VALUES ('steer_fixture', 'capture_db', 'regular', 'nothing is slow', 'sql', 'SELECT 1')",
        )
        .execute(db.handle.pool())
        .await?;
        let run: i64 = sqlx::query_scalar(
            "INSERT INTO wright.check_runs (check_name, source, cadence, started_at, outcome)
             VALUES ('steer_fixture', 'capture_db', 'regular', now(), 'fail') RETURNING id",
        )
        .fetch_one(db.handle.pool())
        .await?;
        for name in NAMES {
            let problem: i64 = sqlx::query_scalar(
                "INSERT INTO wright.problems (check_name, source, scope, resource, state, opened_at)
                 VALUES ('steer_fixture', 'capture_db', 'query', $1, 'open', now())
                 RETURNING id",
            )
            .bind(format!("query: SELECT {name}"))
            .fetch_one(db.handle.pool())
            .await?;
            sqlx::query(
                "INSERT INTO wright.episodes (problem_id, opened_by_run, last_seen_run, evidence)
                 VALUES ($1, $2, $2, '{}'::jsonb)",
            )
            .bind(problem)
            .bind(run)
            .execute(db.handle.pool())
            .await?;
        }
        db.handle.web().assign_wright_causes().await?;
        let mut causes = Vec::new();
        for name in NAMES {
            causes.push(
                sqlx::query_scalar("SELECT cause_id FROM wright.problems WHERE resource = $1")
                    .bind(format!("query: SELECT {name}"))
                    .fetch_one(db.handle.pool())
                    .await?,
            );
        }
        let steers = Steers {
            persist: db.handle.clone(),
        };
        Ok(Self { db, steers, causes })
    }

    pub(super) async fn row(&self, cause: i64) -> anyhow::Result<CauseRow> {
        self.db
            .handle
            .web()
            .wright_cause(cause)
            .await?
            .ok_or_else(|| anyhow::anyhow!("cause {cause} is not in the queue"))
    }

    pub(super) async fn patch(&self, cause: i64, patch: CausePatch) -> anyhow::Result<()> {
        self.db
            .handle
            .web()
            .patch_wright_cause(cause, &patch)
            .await?;
        Ok(())
    }

    pub(super) async fn run(&self, change: &Change) -> anyhow::Result<Applied> {
        let prior = self.steers.read(change).await?;
        let effect = self.steers.apply(1, change).await?;
        Ok(Applied { prior, effect })
    }

    pub(super) async fn undo(
        &self,
        change: &Change,
        applied: &Applied,
    ) -> Result<Value, SteerError> {
        self.steers
            .undo(change, &applied.prior, &applied.effect)
            .await
    }

    pub(super) async fn refusal(&self, change: &Change) -> anyhow::Result<String> {
        match self.steers.read(change).await {
            Err(SteerError::Refused(reason)) => Ok(reason),
            other => anyhow::bail!("{change:?} should be refused, got {other:?}"),
        }
    }

    pub(super) async fn done(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }
}

#[tokio::test]
async fn pin_cause_pins_the_cause_and_undo_unpins_it() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let cause = fx.causes[0];
    let change = Change::PinCause { cause_id: cause };
    let applied = fx.run(&change).await?;
    assert!(fx.row(cause).await?.pinned);
    assert_eq!(applied.prior["pinned"], json!(false));
    fx.undo(&change, &applied).await?;
    assert!(!fx.row(cause).await?.pinned);
    fx.done().await
}

#[tokio::test]
async fn pin_cause_on_a_pinned_cause_leaves_it_pinned_after_undo() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let cause = fx.causes[0];
    fx.patch(
        cause,
        CausePatch {
            pinned: Some(true),
            ..CausePatch::default()
        },
    )
    .await?;
    let change = Change::PinCause { cause_id: cause };
    let applied = fx.run(&change).await?;
    fx.undo(&change, &applied).await?;
    assert!(fx.row(cause).await?.pinned);
    fx.done().await
}

#[tokio::test]
async fn unpin_cause_unpins_and_undo_pins_again() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let cause = fx.causes[1];
    fx.patch(
        cause,
        CausePatch {
            pinned: Some(true),
            ..CausePatch::default()
        },
    )
    .await?;
    let change = Change::UnpinCause { cause_id: cause };
    let applied = fx.run(&change).await?;
    assert!(!fx.row(cause).await?.pinned);
    fx.undo(&change, &applied).await?;
    assert!(fx.row(cause).await?.pinned);
    fx.done().await
}

#[tokio::test]
async fn set_cause_priority_sets_the_order_and_undo_restores_the_previous_value()
-> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let cause = fx.causes[0];
    fx.patch(
        cause,
        CausePatch {
            operator_priority: Some(Some(7.0)),
            ..CausePatch::default()
        },
    )
    .await?;
    let set = Change::SetCausePriority {
        cause_id: cause,
        priority: Some(500.0),
    };
    let applied = fx.run(&set).await?;
    assert_eq!(fx.row(cause).await?.operator_priority, Some(500.0));
    fx.undo(&set, &applied).await?;
    assert_eq!(fx.row(cause).await?.operator_priority, Some(7.0));
    let clear = Change::SetCausePriority {
        cause_id: cause,
        priority: None,
    };
    let cleared = fx.run(&clear).await?;
    assert_eq!(fx.row(cause).await?.operator_priority, None);
    fx.undo(&clear, &cleared).await?;
    assert_eq!(fx.row(cause).await?.operator_priority, Some(7.0));
    fx.done().await
}

#[tokio::test]
async fn set_cause_priority_undo_after_another_change_is_a_conflict() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let cause = fx.causes[0];
    let set = Change::SetCausePriority {
        cause_id: cause,
        priority: Some(500.0),
    };
    let applied = fx.run(&set).await?;
    fx.patch(
        cause,
        CausePatch {
            operator_priority: Some(Some(9.0)),
            ..CausePatch::default()
        },
    )
    .await?;
    let outcome = fx.undo(&set, &applied).await;
    assert!(
        matches!(outcome, Err(SteerError::Conflict(_))),
        "{outcome:?}"
    );
    assert_eq!(fx.row(cause).await?.operator_priority, Some(9.0));
    fx.done().await
}

#[tokio::test]
async fn cause_steers_refuse_an_unknown_cause_with_its_id() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let missing = 987_654_321;
    for change in [
        Change::PinCause { cause_id: missing },
        Change::UnpinCause { cause_id: missing },
        Change::SetCausePriority {
            cause_id: missing,
            priority: Some(1.0),
        },
        Change::MergeCauses {
            into: missing,
            cause_ids: vec![fx.causes[0]],
        },
        Change::MergeCauses {
            into: fx.causes[0],
            cause_ids: vec![missing],
        },
        Change::GroupCauses {
            name: "none".to_owned(),
            cause_ids: vec![fx.causes[0], missing],
        },
        Change::UngroupCauses {
            cause_ids: vec![missing],
        },
    ] {
        let reason = fx.refusal(&change).await?;
        assert!(reason.contains(&missing.to_string()), "{reason}");
        assert!(reason.contains("not in the open triage queue"), "{reason}");
    }
    let reason = fx
        .refusal(&Change::SeparateMerge { merge_id: missing })
        .await?;
    assert!(reason.contains(&missing.to_string()), "{reason}");
    fx.done().await
}

#[tokio::test]
async fn a_conversation_answer_with_a_pin_steer_pins_the_cause_and_undoes_it() -> anyhow::Result<()>
{
    let fx = Fixture::boot().await?;
    let cause = fx.causes[2];
    fx.db.ask("pin-1", "pin the charlie query").await?;
    let engine = FakeEngine::answering(vec![Ok(json!({
        "body": "Pinned it.",
        "steer": {"kind": "pin_cause", "cause_id": cause},
    }))]);
    let steers: Arc<dyn SteerApply> = Arc::new(Steers {
        persist: fx.db.handle.clone(),
    });
    let convo = fx.db.convo(engine.clone(), steers);
    tick(&convo).await?;
    assert!(fx.row(cause).await?.pinned);
    let request = engine
        .requests
        .lock()
        .map_err(|_| anyhow::anyhow!("the request log is poisoned"))?
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("the engine saw no request"))?;
    assert!(
        request.brief.contains("pin_cause {cause_id}"),
        "{}",
        request.brief
    );
    let evidence = request
        .inputs
        .iter()
        .find(|(name, _)| name == "evidence.json")
        .map(|(_, value)| value.clone())
        .ok_or_else(|| anyhow::anyhow!("no evidence.json input"))?;
    let listed: Vec<i64> = evidence["causes"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["cause_id"].as_i64())
                .collect()
        })
        .unwrap_or_default();
    assert!(listed.contains(&cause), "{evidence}");
    assert!(evidence["causes"][0]["resource"].is_string(), "{evidence}");
    assert!(evidence["groups"].is_array(), "{evidence}");
    let steer: i64 = sqlx::query_scalar("SELECT id FROM wright.steers")
        .fetch_one(fx.db.handle.pool())
        .await?;
    steering::undo(&convo, steer, 1).await?;
    assert!(!fx.row(cause).await?.pinned);
    fx.done().await
}
