use crate::store::persistence::eval_repo::EvalPersist as _;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_runs::{LabelSetFilter, LabelSetItem};

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedSet {
    pub items: Vec<LabelSetItem>,
    pub label_ids_hash: String,
    pub labels_as_of: DateTime<Utc>,
}

pub fn label_ids_hash(items: &[LabelSetItem]) -> String {
    let mut ids: Vec<i64> = items
        .iter()
        .flat_map(|i| std::iter::once(i.reaction_label_id).chain(i.delivery_label_id))
        .collect();
    ids.sort_unstable();
    let text = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn stratum_counts(items: &[LabelSetItem]) -> std::collections::BTreeMap<String, usize> {
    let mut out = std::collections::BTreeMap::new();
    for item in items {
        *out.entry(item.stratum.clone()).or_default() += 1;
    }
    out
}

pub async fn load(
    persist: &PersistHandle,
    filter: LabelSetFilter,
    limit: Option<usize>,
) -> anyhow::Result<LoadedSet> {
    let mut items = persist.eval().load_eval_label_set(&filter).await?;
    if let Some(limit) = limit {
        items.truncate(limit);
    }
    Ok(LoadedSet {
        label_ids_hash: label_ids_hash(&items),
        labels_as_of: filter.labels_as_of,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::label_ids_hash;
    use crate::store::persistence::eval_runs::LabelSetItem;

    fn item(turn: i64, reaction: i64, delivery: Option<i64>) -> LabelSetItem {
        LabelSetItem {
            operator_turn_id: turn,
            reaction_label_id: reaction,
            reaction_label: "accepted".to_owned(),
            delivery_label_id: delivery,
            delivery_label: None,
            inclusion_prob: None,
            stratum: "all".to_owned(),
            control: false,
        }
    }

    #[test]
    fn the_hash_ignores_item_order_and_changes_with_the_ids() {
        let a = label_ids_hash(&[item(1, 10, Some(11)), item(2, 20, None)]);
        let b = label_ids_hash(&[item(2, 20, None), item(1, 10, Some(11))]);
        let c = label_ids_hash(&[item(1, 10, Some(12)), item(2, 20, None)]);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
