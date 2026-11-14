use super::*;
use crate::llm::decisions::Answer;

fn correction(id: i64) -> CorrectionForTagging {
    CorrectionForTagging {
        item_id: id,
        corrected_turn_id: 1,
        session_uuid: "s".to_owned(),
        request_id: None,
        qualification: "gold".to_owned(),
        qualification_ref: "ref".to_owned(),
        embedding: String::new(),
    }
}

fn anchor(item_id: i64, pattern: &str) -> AnchorHit {
    AnchorHit {
        item_id,
        pattern: pattern.to_owned(),
        origin_session_uuid: None,
        request_id: None,
        similarity: 0.8,
        score: 0.1,
    }
}

fn row(id: i64) -> NewAssignment {
    NewAssignment {
        corrected_turn_id: 1,
        correction_item_id: id,
        qualification: "gold".to_owned(),
        qualification_ref: "ref".to_owned(),
        pattern: None,
        score: 0.0,
        similarity: 0.0,
        anchor_item_id: None,
        runner_up_pattern: None,
        margin: None,
        top_anchors: json!([]),
        floor_id: 1,
        tagger_version: "tagger-v1".to_owned(),
        embedder_key: "k".to_owned(),
    }
}

fn pick(key: &str, p: f64) -> Answer {
    Answer::Choice {
        choice: key.to_owned(),
        probabilities: BTreeMap::from([(key.to_owned(), p)]),
        confidence: None,
    }
}

#[test]
fn compose_known_pattern_is_tagged_and_none_or_unknown_is_not() {
    let corrections = [correction(1), correction(2), correction(3)];
    let units: Vec<Unit<'_>> = corrections
        .iter()
        .map(|c| Unit {
            correction: c,
            text: "fix it".to_owned(),
            anchors: vec![(anchor(10, "wrong-file"), "an example".to_owned())],
        })
        .collect();
    let asked = Asked::answered(BTreeMap::from([
        (question_key(1), pick("wrong-file", 0.9)),
        (question_key(2), pick(NONE_KEY, 0.7)),
        (question_key(3), pick("invented", 0.9)),
    ]));
    assert_eq!(
        compose(&asked, &units),
        vec![(1, Some("wrong-file".to_owned()), 0.9), (2, None, 0.7)]
    );
}

#[test]
fn compose_failed_answer_leaves_the_anchor_rows_alone() {
    let c = correction(1);
    let units = vec![Unit {
        correction: &c,
        text: String::new(),
        anchors: vec![(anchor(10, "wrong-file"), String::new())],
    }];
    assert!(compose(&Asked::failed_with("timeout"), &units).is_empty());
}

#[test]
fn apply_sets_the_pattern_and_the_matching_anchor() {
    let c = correction(1);
    let units = vec![Unit {
        correction: &c,
        text: String::new(),
        anchors: vec![
            (anchor(10, "other"), String::new()),
            (anchor(11, "wrong-file"), String::new()),
        ],
    }];
    let mut rows = vec![row(1)];
    apply(
        &mut rows,
        &units,
        &[(1, Some("wrong-file".to_owned()), 0.9)],
    );
    assert_eq!(rows[0].pattern.as_deref(), Some("wrong-file"));
    assert_eq!(rows[0].anchor_item_id, Some(11));
}

#[test]
fn questions_offer_the_anchor_patterns_and_none() -> anyhow::Result<()> {
    let c = correction(4);
    let units = vec![Unit {
        correction: &c,
        text: "t".to_owned(),
        anchors: vec![
            (anchor(10, "a"), String::new()),
            (anchor(11, "a"), String::new()),
            (anchor(12, "b"), String::new()),
        ],
    }];
    let asked = questions(&units);
    let Some(Question::Choice { criteria, .. }) = asked.get("correction_4") else {
        anyhow::bail!("expected a choice question");
    };
    let keys: Vec<&str> = criteria.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["a", "b", "none"]);
    assert_eq!(state(&units)["corrections"][0]["correction_item_id"], 4);
    Ok(())
}
