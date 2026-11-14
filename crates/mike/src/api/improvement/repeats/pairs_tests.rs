use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::*;
use crate::llm::decisions::Answer;

fn prompt(turn_id: i64, text: &str) -> PushbackPrompt {
    PushbackPrompt {
        turn_id,
        session_id: Uuid::nil(),
        at: Utc
            .with_ymd_and_hms(2026, 10, 1 + turn_id as u32, 9, 0, 0)
            .unwrap(),
        project_path: "/p/mike".to_owned(),
        text: text.to_owned(),
        embedding: vec![1.0, 0.0],
    }
}

fn answer(p: f64) -> Answer {
    Answer::Noul {
        noul: p,
        confidence: None,
    }
}

#[test]
fn borderline_keeps_only_pairs_near_the_threshold_closest_first() {
    let sim = vec![
        vec![1.0, 0.84, 0.50, 0.80],
        vec![0.84, 1.0, 0.95, 0.10],
        vec![0.50, 0.95, 1.0, 0.0],
        vec![0.80, 0.10, 0.0, 1.0],
    ];
    let pairs = borderline(&sim, 0.84);
    assert_eq!(
        pairs.iter().map(|p| (p.i, p.j)).collect::<Vec<_>>(),
        vec![(0, 1), (0, 3)]
    );
}

#[test]
fn compose_gives_one_probability_per_pair() {
    let asked = Asked::answered(BTreeMap::from([
        (question_key(0), answer(0.9)),
        (question_key(1), answer(0.1)),
    ]));
    assert_eq!(compose(&asked, 2), Some(vec![0.9, 0.1]));
}

#[test]
fn compose_failed_or_short_answer_keeps_the_cosine_threshold() {
    assert_eq!(compose(&Asked::failed_with("timeout"), 2), None);
    let short = Asked::answered(BTreeMap::from([(question_key(0), answer(0.9))]));
    assert_eq!(compose(&short, 2), None);
}

#[tokio::test]
async fn refine_without_a_decider_leaves_the_matrix_untouched() {
    let a = prompt(1, "deploy it");
    let b = prompt(2, "deploy it again");
    let kept = vec![&a, &b];
    let mut sim = vec![vec![1.0, 0.85], vec![0.85, 1.0]];
    refine(None, &kept, &mut sim, 0.84).await;
    assert_eq!(sim, vec![vec![1.0, 0.85], vec![0.85, 1.0]]);
}

#[test]
fn state_and_questions_describe_each_pair() {
    let a = prompt(1, "deploy it");
    let b = prompt(3, "deploy it again");
    let pair = Pair {
        i: 0,
        j: 1,
        cosine: 0.85,
    };
    let value = state(&[(&pair, &a, &b)]);
    assert_eq!(value["pairs"][0]["message_b"], "deploy it again");
    assert_eq!(value["pairs"][0]["days_apart"], 2);
    assert!(questions(1).contains_key("pair_0"));
}
