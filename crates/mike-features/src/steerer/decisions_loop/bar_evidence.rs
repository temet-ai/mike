use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::store::persistence::PersistHandle;

pub(crate) const NEWEST_WRONG: usize = 5;

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub(crate) struct LabelRow {
    pub id: i64,
    pub point: String,
    pub label: String,
    pub label_source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceFigure {
    pub source: String,
    pub labelled: usize,
    pub right: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Evidence {
    pub sources: Vec<SourceFigure>,
    pub newest_wrong: Vec<i64>,
}

pub(crate) fn evidence_for(point: &str, rows: &[LabelRow]) -> Evidence {
    let mut by_source: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut wrong: Vec<i64> = Vec::new();
    for row in rows.iter().filter(|row| row.point == point) {
        let entry = by_source.entry(row.label_source.as_str()).or_default();
        entry.0 += 1;
        if row.label == "right" {
            entry.1 += 1;
        } else {
            wrong.push(row.id);
        }
    }
    wrong.sort_unstable_by(|a, b| b.cmp(a));
    wrong.truncate(NEWEST_WRONG);
    Evidence {
        sources: by_source
            .into_iter()
            .map(|(source, (labelled, right))| SourceFigure {
                source: source.to_owned(),
                labelled,
                right,
            })
            .collect(),
        newest_wrong: wrong,
    }
}

pub(crate) async fn label_rows(
    persist: &PersistHandle,
    since: DateTime<Utc>,
    points: &[String],
) -> Result<Vec<LabelRow>, sqlx::Error> {
    let (retired_points, retired_sources) = crate::llm::decisions::retired::retired_columns();
    sqlx::query_as::<_, LabelRow>(
        "SELECT DISTINCT ON (r.id) r.id, r.point, o.label, o.label_source
           FROM decisions.records r
           JOIN decisions.outcomes o ON o.record_id = r.id
           LEFT JOIN unnest($3::text[], $4::text[]) AS retired(point, label_source)
             ON retired.point = r.point AND retired.label_source = o.label_source
          WHERE r.created_at >= $1 AND r.mode IN ('on', 'shadow', 'replay')
            AND retired.point IS NULL
            AND r.point = ANY($2)
          ORDER BY r.id, o.labelled_at",
    )
    .bind(since)
    .bind(points)
    .bind(retired_points)
    .bind(retired_sources)
    .fetch_all(persist.pool())
    .await
}
