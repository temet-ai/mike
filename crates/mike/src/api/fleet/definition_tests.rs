use axum::response::IntoResponse;
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::definition_patch::{Move, merged, next_move, parse_new};

#[test]
fn clearing_the_dead_run_fields_of_a_starting_seat_is_no_longer_refused() -> anyhow::Result<()> {
    let task = |n: u32| {
        json!({
            "name": format!("task {n}"), "discipline": "backend", "criteria": ["C1"],
            "files": [format!("src/area{n}.rs")], "brief": "b",
            "seat": format!("backend-T{n}"), "session": "1e5d73d0", "state": "starting"
        })
    };
    let stored = json!({"goal": "g", "tasks": {"T1": task(1), "T2": task(2)}});
    for patch in [
        json!({"tasks": {"T1": {"seat": null, "session": null, "state": null}}}),
        json!({"tasks": {"T1": {"session": null}}}),
        json!({"tasks": {"T1": null}}),
        json!({"tasks": {"T2": {"seat": null, "session": null, "state": null}}}),
        json!({"tasks": {"T1": {"name": "Linux journey"}}}),
    ] {
        let saved = merged(&stored, Some(&patch));
        assert!(saved.is_ok(), "{patch}: {:?}", saved.err());
    }
    Ok(())
}
use super::definition_view::{DefinitionError, FleetView, digest_view, seat_view};
use super::definitions::slug_problem;
use crate::launch::fleet::status::{FleetHeader, SeatFacts, build};
use crate::launch::fleet::tell_digest::SlotQueue;
use crate::launch::fleet::{FleetDefinition, Missing};
use crate::store::persistence::PersistError;
use crate::store::persistence::fleet::{FleetHeaderRow, FleetRow};

fn row(state: &str, missing: Value) -> FleetRow {
    let now = Utc::now();
    FleetRow {
        id: 1,
        fleet_uuid: Uuid::nil(),
        slug: "night-run".to_string(),
        project_path: "/repo".to_string(),
        state: state.to_string(),
        definition: json!({"goal": "ship it"}),
        missing,
        run_id: None,
        start_report: None,
        created_by: None,
        created_at: now,
        updated_at: now,
        ready_at: None,
        started_at: None,
        finished_at: None,
    }
}

fn one_missing() -> Vec<Missing> {
    vec![Missing::new("goal", "is empty")]
}

async fn body_of(error: DefinitionError) -> anyhow::Result<(u16, Value)> {
    let response = error.into_response();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

#[test]
fn next_move_marks_a_draft_ready_only_when_asked_and_nothing_is_missing() {
    assert_eq!(next_move("draft", true, &[]), Move::MarkReady);
    assert_eq!(next_move("draft", false, &[]), Move::Stay);
    assert_eq!(next_move("draft", true, &one_missing()), Move::Stay);
}

#[test]
fn next_move_sends_a_ready_fleet_with_new_problems_back_to_draft() {
    assert_eq!(next_move("ready", false, &one_missing()), Move::BackToDraft);
    assert_eq!(next_move("ready", true, &one_missing()), Move::BackToDraft);
    assert_eq!(next_move("ready", true, &[]), Move::Stay);
}

#[test]
fn next_move_leaves_started_fleets_alone() {
    for state in ["starting", "running", "done"] {
        assert_eq!(next_move(state, true, &[]), Move::Stay, "{state}");
        assert_eq!(
            next_move(state, true, &one_missing()),
            Move::Stay,
            "{state}"
        );
    }
}

#[test]
fn next_move_reopens_a_failed_fleet() {
    assert_eq!(next_move("failed", false, &[]), Move::BackToDraft);
    assert_eq!(
        next_move("failed", false, &one_missing()),
        Move::BackToDraft
    );
    assert_eq!(next_move("failed", true, &one_missing()), Move::BackToDraft);
    assert_eq!(next_move("failed", true, &[]), Move::MarkReady);
}

#[test]
fn merged_sets_items_and_keeps_the_ones_the_patch_does_not_name() -> anyhow::Result<()> {
    let stored = json!({"goal": "ship it", "criteria": {"c1": {"text": "a", "check": "b"}}});
    let patch = json!({"criteria": {"c2": {"text": "c", "check": "d"}}});
    let (value, parsed) = merged(&stored, Some(&patch))?;
    assert_eq!(value["goal"], "ship it");
    assert_eq!(value["criteria"]["c1"]["text"], "a");
    assert_eq!(parsed.criteria.len(), 2);
    Ok(())
}

#[test]
fn merged_removes_an_item_the_patch_sets_to_null() -> anyhow::Result<()> {
    let stored = json!({"goal": "ship it", "ask": "please"});
    let (value, parsed) = merged(&stored, Some(&json!({"goal": null})))?;
    assert!(value.get("goal").is_none());
    assert!(parsed.goal.is_empty());
    assert_eq!(parsed.ask, "please");
    Ok(())
}

#[test]
fn merged_without_a_patch_returns_the_stored_definition() -> anyhow::Result<()> {
    let stored = json!({"goal": "ship it"});
    let (value, _) = merged(&stored, None)?;
    assert_eq!(value, stored);
    Ok(())
}

#[test]
fn merged_refuses_a_patch_that_is_not_an_object() {
    let refused = merged(&json!({}), Some(&json!(["goal"])));
    match refused {
        Err(DefinitionError::InvalidDefinition(message)) => {
            assert!(message.contains(r#"["goal"]"#), "{message}");
        }
        other => panic!("expected InvalidDefinition, got {other:?}"),
    }
}

#[test]
fn merged_names_the_deepest_item_with_the_wrong_shape() {
    let patch = json!({"workers": {"rust": {"model": "opus", "contract": 2}}});
    let refused = merged(&json!({}), Some(&patch));
    match refused {
        Err(DefinitionError::InvalidDefinition(message)) => {
            assert!(message.contains("`workers.rust.contract`"), "{message}");
            assert!(message.contains("integer `2`"), "{message}");
        }
        other => panic!("expected InvalidDefinition, got {other:?}"),
    }
}

#[test]
fn merged_names_a_top_level_item_with_the_wrong_type() {
    let refused = merged(&json!({}), Some(&json!({"goal": 5})));
    match refused {
        Err(DefinitionError::InvalidDefinition(message)) => {
            assert!(message.contains("`goal`"), "{message}");
        }
        other => panic!("expected InvalidDefinition, got {other:?}"),
    }
}

#[test]
fn merged_keeps_build_slots_and_refuses_a_count_outside_one_to_sixty_four() -> anyhow::Result<()> {
    let (_, parsed) = merged(&json!({}), Some(&json!({"build_slots": 4})))?;
    assert_eq!(parsed.build_slots, Some(4));
    let (_, unset) = merged(&json!({}), None)?;
    assert_eq!(unset.build_slots, None);
    for wrong in [json!(0), json!(65)] {
        let refused = merged(&json!({}), Some(&json!({"build_slots": wrong})));
        assert!(
            matches!(&refused, Err(DefinitionError::InvalidDefinition(m)) if m.contains("must be from 1 to 64")),
            "{refused:?}"
        );
    }
    let negative = merged(&json!({}), Some(&json!({"build_slots": -1})));
    assert!(
        matches!(&negative, Err(DefinitionError::InvalidDefinition(m)) if m.contains("build_slots")),
        "{negative:?}"
    );
    Ok(())
}

#[test]
fn parse_new_accepts_no_definition_as_an_empty_one() -> anyhow::Result<()> {
    let (value, parsed) = parse_new(None)?;
    assert_eq!(value, json!({}));
    assert!(parsed.tasks.is_empty());
    Ok(())
}

#[test]
fn slug_problem_accepts_plain_slugs() {
    for slug in ["night-run", "fleet_2", "a.b", "X"] {
        assert!(slug_problem(slug).is_none(), "{slug}");
    }
}

#[test]
fn slug_problem_rejects_empty_spaces_slashes_and_fleet_ids() {
    let long = "a".repeat(65);
    let uuid = Uuid::new_v4().to_string();
    for slug in ["", "two words", "a/b", "é", long.as_str(), uuid.as_str()] {
        assert!(slug_problem(slug).is_some(), "{slug}");
    }
}

#[test]
fn view_ready_follows_the_missing_list() -> anyhow::Result<()> {
    let empty = serde_json::to_value(FleetView::full(&row("draft", json!([]))))?;
    assert_eq!(empty["ready"], true);
    let open = json!([{"item": "goal", "problem": "is empty"}]);
    let listed = serde_json::to_value(FleetView::full(&row("draft", open.clone())))?;
    assert_eq!(listed["ready"], false);
    assert_eq!(listed["missing"], open);
    Ok(())
}

#[test]
fn view_lists_without_the_definition_and_shows_it_in_full() -> anyhow::Result<()> {
    let source = row("draft", json!([]));
    let full = serde_json::to_value(FleetView::full(&source))?;
    let listed = serde_json::to_value(FleetView::listed(&FleetHeaderRow::from(&source)))?;
    assert_eq!(full["definition"], json!({"goal": "ship it"}));
    assert!(listed.get("definition").is_none());
    for key in [
        "fleet",
        "slug",
        "project_path",
        "state",
        "run_id",
        "missing",
        "ready",
    ] {
        assert_eq!(full[key], listed[key], "{key}");
    }
    assert_eq!(full["fleet"], Uuid::nil().to_string());
    Ok(())
}

#[tokio::test]
async fn errors_carry_the_named_code_status_and_a_remedy() -> anyhow::Result<()> {
    let cases = [
        (
            DefinitionError::NotFound("x".into()),
            404,
            "fleet_not_found",
        ),
        (DefinitionError::Starting("x".into()), 409, "fleet_starting"),
        (
            DefinitionError::InvalidDefinition("bad".into()),
            400,
            "invalid_definition",
        ),
        (DefinitionError::Changed("x".into()), 409, "fleet_changed"),
        (
            DefinitionError::NotReady {
                slug: "x".into(),
                state: "draft".into(),
            },
            409,
            "fleet_not_ready",
        ),
        (
            DefinitionError::InvalidRequest("bad".into()),
            400,
            "invalid_request",
        ),
        (DefinitionError::Unavailable, 503, "persistence_unavailable"),
        (
            DefinitionError::Persistence(PersistError::DataInvariant("gone".into())),
            500,
            "persistence_failed",
        ),
        (DefinitionError::Status("boom".into()), 500, "status_failed"),
    ];
    for (error, status, code) in cases {
        let (got_status, body) = body_of(error).await?;
        assert_eq!(got_status, status, "{code}");
        assert_eq!(body["error"], code);
        assert!(body["message"].as_str().is_some_and(|m| !m.is_empty()));
    }
    let (_, refused) = body_of(DefinitionError::InvalidDefinition("bad".into())).await?;
    assert!(
        refused["remedy"]
            .as_str()
            .is_some_and(|r| r.contains("Nothing was saved"))
    );
    Ok(())
}

fn twelve_task_definition() -> anyhow::Result<FleetDefinition> {
    let mut tasks = serde_json::Map::new();
    for n in 1..=12u128 {
        tasks.insert(
            format!("T{n}"),
            json!({
                "name": format!("task number {n}"),
                "discipline": "rust-developer",
                "criteria": ["C1"],
                "files": [format!("crates/mike/src/area{n}/one.rs"), format!("crates/mike/src/area{n}/two.rs")],
                "brief": "A long brief that tells the seat what to build and how to prove it, which a peer does not need to read.".repeat(8)
            }),
        );
    }
    FleetDefinition::from_value(&json!({"goal": "g", "tasks": tasks})).map_err(anyhow::Error::msg)
}

fn twelve_sessions() -> std::collections::BTreeMap<String, Uuid> {
    (1..=12u128)
        .map(|n| (format!("T{n}"), Uuid::from_u128(100 + n)))
        .collect()
}

fn tokens_of(value: &Value) -> f64 {
    value.to_string().chars().count() as f64 / 3.5
}

#[test]
fn the_seat_view_of_a_twelve_task_fleet_is_under_2000_tokens_and_holds_no_brief()
-> anyhow::Result<()> {
    let def = twelve_task_definition()?;
    let view = seat_view(
        &def,
        Some(&Uuid::from_u128(103).to_string()),
        &twelve_sessions(),
    );
    assert_eq!(view["you"]["task"], "T3");
    assert!(view["you"]["files"].is_array());
    let peers = view["peers"].as_array().map(Vec::len).unwrap_or_default();
    assert_eq!(peers, 11);
    assert!(
        view["peers"]
            .as_array()
            .is_some_and(|p| p.iter().all(|t| t["task"] != "T3"))
    );
    let text = view.to_string();
    assert!(!text.contains("A long brief"), "{text}");
    assert!(
        !text.contains("area5/one.rs"),
        "a peer's files are not listed"
    );
    assert!(text.contains(&Uuid::from_u128(105).to_string()));
    assert!(tokens_of(&view) < 2000.0, "{} tokens", tokens_of(&view));
    Ok(())
}

#[test]
fn the_seat_view_without_a_known_session_lists_every_task_as_a_peer() -> anyhow::Result<()> {
    let def = twelve_task_definition()?;
    let view = seat_view(&def, Some("not-a-seat"), &twelve_sessions());
    assert!(view["you"].is_null());
    assert_eq!(view["peers"].as_array().map(Vec::len), Some(12));
    Ok(())
}

#[test]
fn the_digest_of_a_twelve_task_fleet_is_under_600_tokens() -> anyhow::Result<()> {
    let def = twelve_task_definition()?;
    let now = Utc::now();
    let seats: Vec<SeatFacts> = (1..=12i32)
        .map(|n| SeatFacts {
            seat: Some(format!("rust-developer-T{n}")),
            session: Some(Uuid::from_u128(200 + u128::from(n.unsigned_abs()))),
            role: Some("rust-developer".into()),
            agent: Some("rust-developer".into()),
            model: Some("sonnet".into()),
            opened_at: Some(now - chrono::Duration::minutes(30)),
            closed_at: None,
            requests: 40,
            last_request_at: Some(now - chrono::Duration::seconds(i64::from(n) * 7)),
            context_tokens: Some(200_000),
            live_state: None,
            last_status: None,
            first_request_cache: None,
        })
        .collect();
    let header = FleetHeader {
        fleet: Uuid::nil().to_string(),
        slug: "night-run".into(),
        state: "running".into(),
        run_id: Some("run".into()),
        started_at: Some(now),
        finished_at: None,
        now,
        missing: Vec::new(),
    };
    let status = build(header, &def, &seats, &[]);
    let digest = digest_view(&status, SlotQueue::default());
    assert!(digest.get("waiting_for_slot").is_none());
    let queued = digest_view(
        &status,
        SlotQueue {
            waiting: 3,
            slots: 2,
        },
    );
    assert_eq!(queued["waiting_for_slot"], json!(3));
    assert_eq!(queued["build_slots"], json!(2));
    let rows = digest["tasks"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 12);
    let first = rows.iter().find(|r| r["task"] == "T1");
    assert_eq!(
        first.map(|r| r["seat"].clone()),
        Some(json!("rust-developer-T1"))
    );
    assert_eq!(first.map(|r| r["idle_seconds"].clone()), Some(json!(7)));
    assert_eq!(first.map(|r| r["passes"].clone()), Some(json!(0)));
    assert_eq!(first.map(|r| r["fails"].clone()), Some(json!(0)));
    assert!(
        tokens_of(&digest) < 600.0,
        "{} tokens: {digest}",
        tokens_of(&digest)
    );
    Ok(())
}
