use crate::steerer::registry::Steerer;
use crate::steerer::rollout_params::Rollout;

pub(in crate::steerer::fleet) fn live_before_shadow<'r>(
    steerers: &[&'r Steerer],
) -> Vec<&'r Steerer> {
    let (mut live, shadow): (Vec<&Steerer>, Vec<&Steerer>) = steerers
        .iter()
        .partition(|s| s.params.delivery.rollout == Rollout::Live);
    live.extend(shadow);
    live
}

pub(in crate::steerer::fleet) fn is_shadow(steerer: &Steerer) -> bool {
    steerer.params.delivery.rollout == Rollout::Shadow
}

#[cfg(test)]
mod tests {
    use super::live_before_shadow;
    use crate::steerer::registry::{Steerer, SteererParams};
    use crate::steerer::rollout_params::Rollout;

    fn steerer(id: i64, rollout: Rollout) -> Steerer {
        let mut params = SteererParams::default();
        params.delivery.rollout = rollout;
        Steerer {
            id,
            uuid: uuid::Uuid::new_v4(),
            name: format!("s{id}"),
            display_name: format!("S{id}"),
            description: String::new(),
            agent_id: id,
            authority: "command".to_owned(),
            mode: "drive".to_owned(),
            model: String::new(),
            priority: i32::try_from(id).unwrap_or(0),
            enabled: true,
            params,
            config: serde_json::json!({}),
            prompt: None,
            kind: "refocus".to_owned(),
            trigger_point: "response".to_owned(),
            invocation: "every_turn".to_owned(),
            pattern: None,
            auto_enrol: false,
            project_id: None,
            project_path: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn order_puts_live_before_shadow_keeping_priority() {
        let a = steerer(1, Rollout::Shadow);
        let b = steerer(2, Rollout::Live);
        let c = steerer(3, Rollout::Shadow);
        let d = steerer(4, Rollout::Live);
        let ordered: Vec<i64> = live_before_shadow(&[&a, &b, &c, &d])
            .iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ordered, vec![2, 4, 1, 3]);
    }

    #[test]
    fn order_all_live_is_identity() {
        let a = steerer(1, Rollout::Live);
        let b = steerer(2, Rollout::Live);
        let ordered: Vec<i64> = live_before_shadow(&[&a, &b]).iter().map(|s| s.id).collect();
        assert_eq!(ordered, vec![1, 2]);
    }
}
