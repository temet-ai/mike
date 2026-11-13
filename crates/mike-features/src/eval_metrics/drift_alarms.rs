use crate::store::persistence::eval_repo::EvalPersist as _;

use crate::semantic::drift_job::DriftLine;
use crate::semantic::knowledge_state::{DriftAlarms, KnowledgeFuture};
use crate::store::persistence::eval_metrics::{AlarmKind, Firing};
use crate::store::persistence::{PersistError, PersistHandle};

const CLEAR_AFTER_QUIET_DAYS: i32 = 3;

pub struct EvalDriftAlarms;

fn firings(embedder_key: &str, lines: &[DriftLine]) -> Vec<Firing> {
    lines
        .iter()
        .filter(|l| l.fired)
        .map(|l| Firing {
            kind: AlarmKind::EmbeddingDrift,
            subject: format!("embedding:{}", l.stream),
            detail: serde_json::json!({
                "day": l.day,
                "embedder_key": embedder_key,
                "distance": l.point.distance,
                "z": l.point.z,
                "n_current": l.n_current,
                "n_baseline": l.n_baseline,
            }),
        })
        .collect()
}

impl DriftAlarms for EvalDriftAlarms {
    fn raise<'a>(
        &'a self,
        persist: &'a PersistHandle,
        embedder_key: &'a str,
        lines: &'a [DriftLine],
    ) -> KnowledgeFuture<'a, Result<(), PersistError>> {
        Box::pin(async move {
            persist
                .eval()
                .apply_alarm_pass(
                    &[AlarmKind::EmbeddingDrift],
                    &firings(embedder_key, lines),
                    CLEAR_AFTER_QUIET_DAYS,
                )
                .await?;
            Ok(())
        })
    }
}
