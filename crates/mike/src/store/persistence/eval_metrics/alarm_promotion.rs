use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};

use super::super::{PersistError, PersistHandle};
use super::types::{AlarmKind, AlarmThresholds, Firing};

const METRIC_DROP_WINDOW_DAYS: i32 = 30;
const METRIC_DROP_PRECISION_FLOOR: f64 = 0.85;
const METRIC_DROP_RECALL_FLOOR: f64 = 0.65;
const REACTIONS: [&str; 5] = [
    "corrected",
    "redirected",
    "accepted",
    "no_response",
    "unclear",
];

#[derive(Debug, Clone, Copy, PartialEq)]
struct BundlePair {
    live_bundle_id: i64,
    shadow_bundle_id: i64,
}

#[derive(Debug, Clone, PartialEq)]
struct ReactionPair {
    live_reaction: String,
    shadow_reaction: String,
    n: i64,
}

#[derive(Debug, Clone, PartialEq)]
struct MetricDropRow {
    subject: String,
    detail: Value,
    fires: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct KappaStats {
    n: i64,
    kappa: f64,
    confusion: Value,
}

fn reaction_index(reaction: &str) -> Option<usize> {
    REACTIONS.iter().position(|r| *r == reaction)
}

fn confusion_json(matrix: &[[i64; 5]; 5]) -> Value {
    let mut rows = Map::new();
    for (i, live) in REACTIONS.iter().enumerate() {
        let mut cols = Map::new();
        for (j, shadow) in REACTIONS.iter().enumerate() {
            cols.insert((*shadow).to_owned(), json!(matrix[i][j]));
        }
        rows.insert((*live).to_owned(), Value::Object(cols));
    }
    Value::Object(rows)
}

fn cohens_kappa(pairs: &[ReactionPair]) -> KappaStats {
    let mut matrix = [[0i64; 5]; 5];
    let mut total: i64 = 0;
    for pair in pairs {
        if let (Some(i), Some(j)) = (
            reaction_index(&pair.live_reaction),
            reaction_index(&pair.shadow_reaction),
        ) {
            matrix[i][j] += pair.n;
            total += pair.n;
        }
    }
    if total == 0 {
        return KappaStats {
            n: 0,
            kappa: 0.0,
            confusion: confusion_json(&matrix),
        };
    }
    let n_f = f64::from(u32::try_from(total).unwrap_or(u32::MAX));
    let mut agree = 0.0;
    let mut chance = 0.0;
    for (i, row) in matrix.iter().enumerate() {
        agree += row[i] as f64;
        let row_sum: i64 = row.iter().sum();
        let col_sum: i64 = matrix.iter().map(|r| r[i]).sum();
        chance += (row_sum as f64 / n_f) * (col_sum as f64 / n_f);
    }
    let po = agree / n_f;
    let kappa = if (1.0 - chance).abs() < f64::EPSILON {
        if (po - 1.0).abs() < f64::EPSILON {
            1.0
        } else {
            0.0
        }
    } else {
        (po - chance) / (1.0 - chance)
    };
    KappaStats {
        n: total,
        kappa,
        confusion: confusion_json(&matrix),
    }
}

fn shadow_disagreement_firing(
    live_bundle_id: i64,
    shadow_bundle_id: i64,
    stats: &KappaStats,
) -> Firing {
    Firing {
        kind: AlarmKind::ShadowDisagreement,
        subject: format!("turn_judge:{live_bundle_id}"),
        detail: json!({
            "n": stats.n,
            "kappa": stats.kappa,
            "confusion": stats.confusion,
            "live_bundle_id": live_bundle_id,
            "shadow_bundle_id": shadow_bundle_id,
        }),
    }
}

impl PersistHandle {
    async fn eval_shadow_bundle_pair(&self) -> Result<Option<(i64, i64)>, PersistError> {
        crate::metrics::time_db("eval_alarm_shadow_bundle_pair", async move {
            let row = sqlx::query_as!(
                BundlePair,
                r#"WITH cfg AS (
                       SELECT coalesce(max((value #>> '{}')::bigint), 0) AS live_id
                         FROM config.settings
                        WHERE key = 'turn_judge.live_bundle_id'
                   ),
                   shadow AS (
                       SELECT id, components FROM eval.bundles
                        WHERE role = 'turn_judge_shadow' AND origin = 'resolver'
                          AND superseded_at IS NULL
                        ORDER BY created_at DESC, id DESC
                        LIMIT 1
                   ),
                   live AS (
                       SELECT b.id, b.components FROM eval.bundles b JOIN cfg ON b.id = cfg.live_id
                   )
                   SELECT live.id AS "live_bundle_id!", shadow.id AS "shadow_bundle_id!"
                     FROM live, shadow
                    WHERE (live.components - 'role'::text) = (shadow.components - 'role'::text)"#,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| (r.live_bundle_id, r.shadow_bundle_id)))
        })
        .await
    }

    async fn eval_shadow_reaction_pairs(
        &self,
        live_bundle_id: i64,
        shadow_bundle_id: i64,
        at: DateTime<Utc>,
    ) -> Result<Vec<ReactionPair>, PersistError> {
        crate::metrics::time_db("eval_alarm_shadow_reaction_pairs", async move {
            Ok(sqlx::query_as!(
                ReactionPair,
                r#"SELECT lv.reaction AS "live_reaction!", sv.reaction AS "shadow_reaction!",
                          count(*)::bigint AS "n!"
                     FROM memory.turn_verdicts lv
                     JOIN memory.turn_verdicts sv ON sv.operator_turn_id = lv.operator_turn_id
                    WHERE lv.mode = 'live' AND lv.bundle_id = $1
                      AND sv.mode = 'shadow' AND sv.bundle_id = $2
                      AND lv.reaction IS NOT NULL AND sv.reaction IS NOT NULL
                      AND lv.created_at >= $3::timestamptz - interval '14 days'
                      AND lv.created_at < $3::timestamptz
                    GROUP BY lv.reaction, sv.reaction"#,
                live_bundle_id,
                shadow_bundle_id,
                at,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    async fn eval_shadow_disagreement(
        &self,
        at: DateTime<Utc>,
        min_n: i64,
        min_kappa: f64,
    ) -> Result<Vec<Firing>, PersistError> {
        let Some((live_bundle_id, shadow_bundle_id)) = self.eval_shadow_bundle_pair().await? else {
            return Ok(Vec::new());
        };
        let pairs = self
            .eval_shadow_reaction_pairs(live_bundle_id, shadow_bundle_id, at)
            .await?;
        let stats = cohens_kappa(&pairs);
        if stats.n >= min_n && stats.kappa < min_kappa {
            return Ok(vec![shadow_disagreement_firing(
                live_bundle_id,
                shadow_bundle_id,
                &stats,
            )]);
        }
        Ok(Vec::new())
    }

    async fn eval_metric_drop_rows(
        &self,
        at: DateTime<Utc>,
        min_n: i64,
    ) -> Result<Vec<MetricDropRow>, PersistError> {
        crate::metrics::time_db("eval_alarm_metric_drop", async move {
            Ok(sqlx::query_file_as!(
                MetricDropRow,
                "src/store/persistence/eval_metrics/metric_drop.sql",
                at,
                METRIC_DROP_WINDOW_DAYS,
                min_n,
                METRIC_DROP_PRECISION_FLOOR,
                METRIC_DROP_RECALL_FLOOR,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    async fn eval_metric_drop(
        &self,
        at: DateTime<Utc>,
        min_n: i64,
    ) -> Result<Vec<Firing>, PersistError> {
        Ok(self
            .eval_metric_drop_rows(at, min_n)
            .await?
            .into_iter()
            .filter(|row| row.fires)
            .map(|row| Firing {
                kind: AlarmKind::MetricDrop,
                subject: row.subject,
                detail: row.detail,
            })
            .collect())
    }

    pub async fn evaluate_promotion_alarms(
        &self,
        t: &AlarmThresholds,
        at: DateTime<Utc>,
    ) -> Result<Vec<Firing>, PersistError> {
        let mut out = Vec::new();
        if t.shadow_disagreement_enabled {
            out.extend(
                self.eval_shadow_disagreement(
                    at,
                    t.shadow_disagreement_min_n,
                    t.shadow_disagreement_min_kappa,
                )
                .await?,
            );
        }
        if t.metric_drop_enabled {
            out.extend(self.eval_metric_drop(at, t.metric_drop_min_n).await?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{ReactionPair, cohens_kappa};

    fn pair(live: &str, shadow: &str, n: i64) -> ReactionPair {
        ReactionPair {
            live_reaction: live.to_owned(),
            shadow_reaction: shadow.to_owned(),
            n,
        }
    }

    #[test]
    fn cohens_kappa_perfect_agreement_is_one() {
        let pairs = vec![
            pair("corrected", "corrected", 30),
            pair("accepted", "accepted", 20),
        ];
        let stats = cohens_kappa(&pairs);
        assert_eq!(stats.n, 50);
        assert!((stats.kappa - 1.0).abs() < 1e-9, "{}", stats.kappa);
    }

    #[test]
    fn cohens_kappa_chance_agreement_is_near_zero() {
        let pairs = vec![
            pair("corrected", "corrected", 25),
            pair("corrected", "accepted", 25),
            pair("accepted", "corrected", 25),
            pair("accepted", "accepted", 25),
        ];
        let stats = cohens_kappa(&pairs);
        assert_eq!(stats.n, 100);
        assert!(stats.kappa.abs() < 1e-9, "{}", stats.kappa);
    }

    #[test]
    fn cohens_kappa_empty_pairs_is_zero_with_zero_n() {
        let stats = cohens_kappa(&[]);
        assert_eq!(stats.n, 0);
        assert_eq!(stats.kappa, 0.0);
    }

    #[test]
    fn cohens_kappa_partial_disagreement_between_zero_and_one() {
        let pairs = vec![
            pair("corrected", "corrected", 40),
            pair("corrected", "redirected", 10),
            pair("redirected", "redirected", 20),
            pair("accepted", "accepted", 30),
        ];
        let stats = cohens_kappa(&pairs);
        assert_eq!(stats.n, 100);
        assert!(stats.kappa > 0.0 && stats.kappa < 1.0, "{}", stats.kappa);
    }
}
