use chrono::{TimeZone, Utc};
use serde_json::json;

use super::*;
use crate::llm::decisions::Answer;

fn neighbour(id: i64) -> NeighbourLesson {
    NeighbourLesson {
        id,
        text: format!("stored lesson {id}"),
        distance: 0.1,
        source: "operator".to_owned(),
        created_at: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
        source_turn_id: None,
        session_id: None,
    }
}

fn document(path: &str) -> GuidanceRow {
    GuidanceRow {
        reference: format!("ref:{path}"),
        source_name: "rules".to_owned(),
        relative_path: path.to_owned(),
        title: "A rule".to_owned(),
        body: String::new(),
        metadata: json!({}),
        similarity: None,
    }
}

fn pick(key: &str, p: f64) -> Answer {
    Answer::Choice {
        choice: key.to_owned(),
        probabilities: BTreeMap::from([(key.to_owned(), p)]),
        confidence: None,
    }
}

fn answered(relation: &str, p: f64, home: &str) -> Asked {
    Asked::answered(BTreeMap::from([
        (relation_key(1), pick(relation, p)),
        (HOME_KEY.to_owned(), pick(home, 0.8)),
        (
            PERSONA_KEY.to_owned(),
            Answer::Noul {
                noul: 0.1,
                confidence: None,
            },
        ),
    ]))
}

#[test]
fn compose_update_relation_asks_for_replacement_text() -> anyhow::Result<()> {
    let answer = compose(&answered("update", 0.9, "memory"), &[neighbour(1)], &[])
        .ok_or_else(|| anyhow::anyhow!("expected an answer"))?;
    assert_eq!(answer.decisions[0].decision, Relation::Update);
    assert!(needs_replacement(&answer));
    assert_eq!(answer.placement.map(|p| p.home), Some(Home::Memory));
    Ok(())
}

#[test]
fn compose_unsure_skip_becomes_update() -> anyhow::Result<()> {
    let answer = compose(&answered("skip", 0.4, "memory"), &[neighbour(1)], &[])
        .ok_or_else(|| anyhow::anyhow!("expected an answer"))?;
    assert_eq!(answer.decisions[0].decision, Relation::Update);
    Ok(())
}

#[test]
fn compose_confident_skip_stays_skip_without_replacement() -> anyhow::Result<()> {
    let answer = compose(&answered("skip", 0.9, "memory"), &[neighbour(1)], &[])
        .ok_or_else(|| anyhow::anyhow!("expected an answer"))?;
    assert_eq!(answer.decisions[0].decision, Relation::Skip);
    assert!(!needs_replacement(&answer));
    Ok(())
}

#[test]
fn compose_failed_or_missing_answers_give_the_model_path() {
    assert!(compose(&Asked::failed_with("timeout"), &[neighbour(1)], &[]).is_none());
    let partial = Asked::answered(BTreeMap::from([(HOME_KEY.to_owned(), pick("memory", 0.9))]));
    assert!(compose(&partial, &[neighbour(1)], &[]).is_none());
}

#[test]
fn placement_from_keeps_only_a_shown_document_and_the_persona_flag() -> anyhow::Result<()> {
    let shown = vec![document("rules/deploy.md")];
    let asked = Asked::answered(BTreeMap::from([
        (HOME_KEY.to_owned(), pick("nowhere", 0.9)),
        (COVERED_KEY.to_owned(), pick("rules/deploy.md", 0.8)),
        (
            PERSONA_KEY.to_owned(),
            Answer::Noul {
                noul: 0.9,
                confidence: None,
            },
        ),
    ]));
    let placement =
        placement_from(&asked, &shown).ok_or_else(|| anyhow::anyhow!("expected a placement"))?;
    assert_eq!(placement.home, Home::Nowhere);
    assert_eq!(placement.covered_by, "rules/deploy.md");
    assert!(placement.persona);
    Ok(())
}

#[test]
fn jev_without_an_environment_is_absent_so_the_model_path_runs() {
    assert!(Jev::new(None).is_none());
}

#[test]
fn questions_cover_every_neighbour_the_home_and_the_documents() {
    let shown = vec![document("rules/deploy.md")];
    let asked = questions(&[neighbour(1), neighbour(2)], &shown);
    for key in ["relation_1", "relation_2", "home", "persona", "covered_by"] {
        assert!(asked.contains_key(key), "missing {key}");
    }
    let value = state("new rule", &[neighbour(1)], &shown);
    assert_eq!(value["stored_lessons"][0]["lesson_id"], 1);
    assert_eq!(value["shown_documents"][0]["path"], "rules/deploy.md");
}
