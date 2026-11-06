use serde::Serialize;
use sqlx::{Postgres, QueryBuilder};

use super::super::{PersistError, PersistHandle};
use super::{Disagreement, EvalRunItemRow, RunItemWrite, RunItemsFilter};

const ITEM_COLUMNS: &str = "SELECT i.run_id, i.operator_turn_id, i.reaction_label_id,
        i.delivery_label_id, i.turn_verdict_id, i.outcome, i.stratum, i.inclusion_prob,
        i.reaction_correct, i.delivery_correct, i.created_at,
        rl.label AS gold_reaction, dl.label AS gold_delivery,
        coalesce(rl.provenance -> 'control' = 'true'::jsonb, false) AS gold_control,
        v.reaction AS verdict_reaction, v.delivery AS verdict_delivery,
        v.legacy_label AS verdict_legacy_label, f.next_state,";

const ITEM_JOINS: &str = "
   FROM eval.run_items i
   LEFT JOIN eval.labels rl ON rl.id = i.reaction_label_id
   LEFT JOIN eval.labels dl ON dl.id = i.delivery_label_id
   LEFT JOIN memory.turn_verdicts v ON v.id = i.turn_verdict_id
   LEFT JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id";

fn push_cur_cte(qb: &mut QueryBuilder<Postgres>, run_id: i64) {
    qb.push(
        "cur AS (
    SELECT DISTINCT ON (c.subject_id, c.axis) c.subject_id, c.axis, c.label
      FROM eval.labels_current_v c
     WHERE c.subject_kind = 'turn' AND c.axis IN ('reaction', 'delivery')
       AND c.tier IN ('gold', 'relayed', 'seed')
       AND c.subject_id IN (SELECT operator_turn_id FROM eval.run_items WHERE run_id = ",
    )
    .push_bind(run_id)
    .push(
        ")
     ORDER BY c.subject_id, c.axis,
              CASE c.tier WHEN 'gold' THEN 1 WHEN 'relayed' THEN 2 ELSE 3 END,
              c.created_at DESC, c.id DESC
)",
    );
}

fn push_rev_cte(qb: &mut QueryBuilder<Postgres>, run_id: i64) {
    qb.push(
        "rev AS (
    SELECT l.operator_turn_id, max(l.created_at) AS reviewed_at
      FROM eval.labels l
     WHERE l.subject_kind = 'turn' AND l.tier = 'gold' AND l.action = 'assert'
       AND l.provenance ->> 'eval_run_id' IS NOT NULL
       AND l.operator_turn_id IN (SELECT operator_turn_id FROM eval.run_items WHERE run_id = ",
    )
    .push_bind(run_id)
    .push(
        ")
     GROUP BY l.operator_turn_id
)",
    );
}

fn items_query(run_id: i64, f: &RunItemsFilter) -> QueryBuilder<Postgres> {
    let with_review = f.include_review;
    let with_rev = with_review || f.reviewed.is_some();
    let mut qb = QueryBuilder::new("");
    if with_rev {
        qb.push("WITH ");
        if with_review {
            push_cur_cte(&mut qb, run_id);
            qb.push(", ");
        }
        push_rev_cte(&mut qb, run_id);
        qb.push(" ");
    }
    qb.push(ITEM_COLUMNS);
    qb.push(if f.include_verdict {
        " to_jsonb(v)"
    } else {
        " NULL::jsonb"
    });
    qb.push(" AS verdict, ");
    qb.push(if with_review {
        "rl.severity"
    } else {
        "NULL::text"
    });
    qb.push(" AS gold_severity, ");
    qb.push(if with_review {
        "cr.label"
    } else {
        "NULL::text"
    });
    qb.push(" AS current_reaction, ");
    qb.push(if with_review {
        "cd.label"
    } else {
        "NULL::text"
    });
    qb.push(" AS current_delivery, ");
    qb.push(if with_review {
        "rev.reviewed_at"
    } else {
        "NULL::timestamptz"
    });
    qb.push(" AS reviewed_at, ");
    qb.push(if with_review {
        "s.session_uuid"
    } else {
        "NULL::uuid"
    });
    qb.push(" AS session_uuid, ");
    qb.push(if with_review { "s.title" } else { "NULL::text" });
    qb.push(" AS session_title, count(*) OVER () AS total, ");
    qb.push(if with_rev {
        "count(rev.reviewed_at) OVER ()"
    } else {
        "0::bigint"
    });
    qb.push(" AS reviewed_total");
    qb.push(ITEM_JOINS);
    if with_review {
        qb.push(
            "
   LEFT JOIN cur cr ON cr.subject_id = i.operator_turn_id AND cr.axis = 'reaction'
   LEFT JOIN cur cd ON cd.subject_id = i.operator_turn_id AND cd.axis = 'delivery'
   LEFT JOIN memory.operator_turns ot ON ot.id = i.operator_turn_id
   LEFT JOIN sessions.sessions s ON s.id = ot.session_id",
        );
    }
    if with_rev {
        qb.push("\n   LEFT JOIN rev ON rev.operator_turn_id = i.operator_turn_id");
    }
    qb.push("\n  WHERE i.run_id = ").push_bind(run_id);
    push_item_filters(&mut qb, f);
    qb.push(" ORDER BY i.operator_turn_id LIMIT ")
        .push_bind(f.limit)
        .push(" OFFSET ")
        .push_bind(f.offset);
    qb
}

fn push_item_filters(qb: &mut QueryBuilder<Postgres>, f: &RunItemsFilter) {
    if let Some(outcome) = &f.outcome {
        qb.push(" AND i.outcome = ").push_bind(outcome);
    }
    if let Some(correct) = f.reaction_correct {
        qb.push(" AND i.reaction_correct = ").push_bind(correct);
    }
    if let Some(correct) = f.delivery_correct {
        qb.push(" AND i.delivery_correct = ").push_bind(correct);
    }
    match f.disagreement {
        Some(Disagreement::Reaction) => {
            qb.push(" AND i.reaction_correct = false");
        }
        Some(Disagreement::Delivery) => {
            qb.push(" AND i.delivery_correct = false");
        }
        Some(Disagreement::Any) => {
            qb.push(" AND (i.reaction_correct = false OR i.delivery_correct = false)");
        }
        None => {}
    }
    if let Some(label) = &f.labels.gold_reaction {
        qb.push(" AND rl.label = ").push_bind(label);
    }
    if let Some(label) = &f.labels.gold_delivery {
        qb.push(" AND dl.label = ").push_bind(label);
    }
    if let Some(label) = &f.labels.verdict_reaction {
        qb.push(" AND v.reaction = ").push_bind(label);
    }
    if let Some(label) = &f.labels.verdict_delivery {
        qb.push(" AND v.delivery = ").push_bind(label);
    }
    if let Some(severity) = &f.labels.severity {
        qb.push(" AND rl.severity = ").push_bind(severity);
    }
    match f.reviewed {
        Some(true) => {
            qb.push(" AND rev.reviewed_at IS NOT NULL");
        }
        Some(false) => {
            qb.push(" AND rev.reviewed_at IS NULL");
        }
        None => {}
    }
    if let Some(turn_id) = f.operator_turn_id {
        qb.push(" AND i.operator_turn_id = ").push_bind(turn_id);
    }
}

#[derive(sqlx::FromRow)]
struct Counted {
    #[sqlx(flatten)]
    row: EvalRunItemRow,
    total: i64,
    reviewed_total: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunItemsPage {
    pub items: Vec<EvalRunItemRow>,
    pub total: i64,
    pub reviewed: i64,
}

impl PersistHandle {
    pub async fn upsert_eval_run_item(&self, item: &RunItemWrite<'_>) -> Result<(), PersistError> {
        crate::metrics::time_db("upsert_eval_run_item", async move {
            sqlx::query!(
                "INSERT INTO eval.run_items (run_id, operator_turn_id, reaction_label_id,
                        delivery_label_id, turn_verdict_id, outcome, stratum, inclusion_prob,
                        reaction_correct, delivery_correct)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT (run_id, operator_turn_id) DO UPDATE
                    SET turn_verdict_id = EXCLUDED.turn_verdict_id, outcome = EXCLUDED.outcome,
                        reaction_correct = EXCLUDED.reaction_correct,
                        delivery_correct = EXCLUDED.delivery_correct",
                item.run_id,
                item.operator_turn_id,
                item.reaction_label_id,
                item.delivery_label_id,
                item.turn_verdict_id,
                item.outcome,
                item.stratum,
                item.inclusion_prob,
                item.reaction_correct,
                item.delivery_correct,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn eval_run_items_page(
        &self,
        run_id: i64,
        f: &RunItemsFilter,
    ) -> Result<RunItemsPage, PersistError> {
        crate::metrics::time_db("list_eval_run_items", async move {
            let rows = items_query(run_id, f)
                .build_query_as::<Counted>()
                .fetch_all(&self.pool)
                .await?;
            let (total, reviewed) = rows.first().map_or((0, 0), |r| (r.total, r.reviewed_total));
            Ok(RunItemsPage {
                items: rows.into_iter().map(|r| r.row).collect(),
                total,
                reviewed,
            })
        })
        .await
    }

    pub async fn list_eval_run_items(
        &self,
        run_id: i64,
        f: &RunItemsFilter,
    ) -> Result<(Vec<EvalRunItemRow>, i64), PersistError> {
        let page = self.eval_run_items_page(run_id, f).await?;
        Ok((page.items, page.total))
    }

    pub async fn all_eval_run_items(
        &self,
        run_id: i64,
    ) -> Result<Vec<EvalRunItemRow>, PersistError> {
        let filter = RunItemsFilter {
            limit: i64::MAX,
            ..RunItemsFilter::default()
        };
        Ok(self.list_eval_run_items(run_id, &filter).await?.0)
    }
}
