use super::super::record::{ActorRef, EvaluationScope};
use super::support::make_verifier_steerer;
use crate::steerer::fleet::intervention::{DeliveryRoute, addressing};
use crate::steerer::rollout_params::{RolloutParams, SteererClass};

fn scope(actor: Option<ActorRef>) -> EvaluationScope {
    EvaluationScope {
        session_db_id: 1,
        operator_turn_id: 2,
        trigger_point: "response",
        trigger_request_id: Some(3),
        actor,
    }
}

#[test]
fn efficiency_steerer_is_appended_not_pushed() {
    let mut steerer = make_verifier_steerer();
    steerer.params.delivery = RolloutParams::from_json(&serde_json::json!({"class": "efficiency"}));
    let a = addressing(&steerer, Some(&scope(None)));
    assert_eq!(a.route, DeliveryRoute::Append);
    assert_eq!(a.addressed_to, "session");
    assert_eq!(steerer.params.delivery.min_interval_secs, 1_800);
}

#[test]
fn correction_steerer_keeps_the_push() {
    let steerer = make_verifier_steerer();
    assert_eq!(steerer.params.delivery.class, SteererClass::Correction);
    assert_eq!(
        addressing(&steerer, Some(&scope(None))).route,
        DeliveryRoute::Push
    );
}

#[test]
fn an_agent_id_trigger_addresses_the_parent() {
    let steerer = make_verifier_steerer();
    let actor = ActorRef {
        persona: Some("rust-developer".to_owned()),
        agent_id: Some("a1b2c3".to_owned()),
    };
    let a = addressing(&steerer, Some(&scope(Some(actor))));
    assert_eq!(a.route, DeliveryRoute::Append);
    assert_eq!(a.addressed_to, "parent_for_actor");
    let prefix = a.actor_prefix.unwrap_or_default();
    assert!(prefix.starts_with("For the agent you dispatched (rust-developer a1b2c3)"));
}

#[test]
fn an_unmet_binary_requirement_is_not_on_path() {
    assert!(!super::super::delivery_gate::binary_on_path(
        "ast-grep-missing-for-this-test"
    ));
}
