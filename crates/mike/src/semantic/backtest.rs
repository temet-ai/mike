use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;

use crate::dossier::view::types::{AsOf, AsOfMode};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::semantic::{EmbedderRow, TaggerBacktestCase, parse_vector};

use super::SemanticError;
use super::config::SemanticConfig;
use super::drift_job::{DriftLine, DriftRun};
use super::embedder::cosine;
use super::floors_job::floor_set;
use super::neighbours::Retriever;
use super::retrieve::{Audience, QuerySubject, RetrievedKind, Scope, SemanticQuery};

const RETRIEVAL_SUBJECTS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RetrievalBacktest {
    pub subjects: usize,
    pub non_empty_operator: usize,
    pub non_empty_project: usize,
    pub operator_only: usize,
    pub example_operator_only_turns: Vec<i64>,
}

pub async fn retrieval(
    persist: &PersistHandle,
    config: &SemanticConfig,
    embedder: &EmbedderRow,
    window: (DateTime<Utc>, DateTime<Utc>),
) -> Result<RetrievalBacktest, SemanticError> {
    let turns = persist
        .semantic_turns_with_items(
            &embedder.embedder_key,
            window.0,
            window.1,
            RETRIEVAL_SUBJECTS,
        )
        .await?;
    let retriever = Retriever {
        persist,
        embedding: None,
        config,
        live_consumer: false,
    };
    let mut out = RetrievalBacktest {
        subjects: turns.len(),
        non_empty_operator: 0,
        non_empty_project: 0,
        operator_only: 0,
        example_operator_only_turns: Vec::new(),
    };
    for (turn, closed_at) in turns {
        let query = |scope| SemanticQuery {
            subject: QuerySubject::ClosedTurn(turn),
            audience: Audience::Internal,
            kinds: vec![RetrievedKind::Correction],
            scope,
            as_of: AsOf {
                at: closed_at,
                mode: AsOfMode::Reconstructed,
            },
            k: 1,
            offset: 0,
        };
        let operator = retriever.neighbours(&query(Scope::Operator)).await?.total > 0;
        let project = retriever.neighbours(&query(Scope::Project)).await?.total > 0;
        out.non_empty_operator += usize::from(operator);
        out.non_empty_project += usize::from(project);
        if operator && !project {
            out.operator_only += 1;
            out.example_operator_only_turns.push(turn);
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaggerBacktest {
    pub anchors: usize,
    pub patterns: usize,
    pub top1_correct: usize,
    pub assigned: usize,
    pub assigned_correct: usize,
    pub coverage: f64,
    pub accuracy_when_assigned: Option<f64>,
}

struct Outcome {
    top_right: bool,
    assigned: bool,
    assigned_right: bool,
}

fn leave_one_session_out(
    case: &TaggerBacktestCase,
    all: &[(TaggerBacktestCase, Vec<f32>)],
    floor: f64,
    min_score: f64,
) -> Option<Outcome> {
    let own = parse_vector(&case.embedding);
    let mut hits: Vec<(&TaggerBacktestCase, f64)> = all
        .iter()
        .filter(|(other, _)| {
            other.item_id != case.item_id
                && other.origin_session_uuid != case.origin_session_uuid
                && (case.request_id.is_none() || other.request_id != case.request_id)
        })
        .map(|(other, vector)| (other, cosine(&own, vector) - floor))
        .collect();
    hits.sort_by(|(a, a_score), (b, b_score)| {
        b_score.total_cmp(a_score).then(a.item_id.cmp(&b.item_id))
    });
    let (top, score) = hits.first()?;
    let top_right = top
        .patterns
        .iter()
        .any(|pattern| case.patterns.contains(pattern));
    let assigned = *score >= min_score;
    Some(Outcome {
        top_right,
        assigned,
        assigned_right: assigned && top_right,
    })
}

pub async fn tagger(
    persist: &PersistHandle,
    config: &SemanticConfig,
    embedder: &EmbedderRow,
) -> Result<TaggerBacktest, SemanticError> {
    let floors = floor_set(persist, &embedder.embedder_key, config.floor_quantile).await?;
    let floor = floors.by_pair.get("corr:corr").map_or(0.0, |f| f.floor);
    let anchors = persist.semantic_anchors(&embedder.embedder_key).await?;
    let cases = persist
        .semantic_tagger_backtest_cases(&embedder.embedder_key)
        .await?;
    let with_vectors: Vec<(TaggerBacktestCase, Vec<f32>)> = cases
        .iter()
        .map(|a| (a.clone(), parse_vector(&a.embedding)))
        .collect();
    let patterns = anchors
        .iter()
        .map(|a| a.pattern.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let (mut top1, mut assigned, mut assigned_correct) = (0, 0, 0);
    for case in &cases {
        let Some(o) = leave_one_session_out(case, &with_vectors, floor, config.tagger_min_score)
        else {
            continue;
        };
        top1 += usize::from(o.top_right);
        assigned += usize::from(o.assigned);
        assigned_correct += usize::from(o.assigned_right);
    }
    let n = cases.len().max(1) as f64;
    Ok(TaggerBacktest {
        anchors: anchors.len(),
        patterns,
        top1_correct: top1,
        assigned,
        assigned_correct,
        coverage: assigned as f64 / n,
        accuracy_when_assigned: (assigned > 0).then(|| assigned_correct as f64 / assigned as f64),
    })
}

pub async fn drift(
    persist: &PersistHandle,
    config: &SemanticConfig,
    embedder: &EmbedderRow,
    days: (NaiveDate, NaiveDate),
) -> Result<Vec<DriftLine>, SemanticError> {
    let run = DriftRun {
        persist,
        config,
        embedder,
        write: false,
    };
    let mut out = Vec::new();
    let mut day = days.0;
    while day <= days.1 {
        out.extend(run.day(day).await?);
        day += Duration::days(1);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{TaggerBacktestCase, leave_one_session_out};

    fn case(id: i64, session: &str, patterns: &[&str], vector: &str) -> TaggerBacktestCase {
        TaggerBacktestCase {
            item_id: id,
            patterns: patterns.iter().map(|p| (*p).to_owned()).collect(),
            origin_session_uuid: Some(session.to_owned()),
            request_id: Some(id),
            embedding: vector.to_owned(),
        }
    }

    #[test]
    fn tagger_backtest_scores_unique_multilabel_corrections() -> anyhow::Result<()> {
        let subject = case(1, "session-a", &["alpha", "beta"], "[1,0]");
        let same_session = case(2, "session-a", &["wrong"], "[1,0]");
        let other_session = case(3, "session-b", &["beta", "gamma"], "[0.99,0.01]");
        let rows = vec![
            (same_session, vec![1.0, 0.0]),
            (other_session, vec![0.99, 0.01]),
        ];
        let outcome = leave_one_session_out(&subject, &rows, 0.90, 0.05)
            .ok_or_else(|| anyhow::anyhow!("expected a backtest outcome"))?;
        assert!(outcome.top_right);
        assert!(outcome.assigned);
        assert!(outcome.assigned_right);
        Ok(())
    }
}
