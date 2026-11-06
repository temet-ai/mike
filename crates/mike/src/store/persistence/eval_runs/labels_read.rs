use chrono::{DateTime, Utc};

use super::super::{PersistError, PersistHandle};
use super::LabelSetItem;

#[derive(Debug, Clone, PartialEq)]
pub struct LabelSetFilter {
    pub labels_as_of: DateTime<Utc>,
    pub split: String,
    pub tiers: Vec<String>,
    pub methods: Vec<String>,
    pub holdout_batch: Option<String>,
}

impl PersistHandle {
    pub async fn load_eval_label_set(
        &self,
        f: &LabelSetFilter,
    ) -> Result<Vec<LabelSetItem>, PersistError> {
        crate::metrics::time_db("load_eval_label_set", async move {
            Ok(match f.holdout_batch.as_deref() {
                Some(batch) => {
                    sqlx::query_file_as!(
                        LabelSetItem,
                        "src/store/persistence/eval_runs/label_set_batch.sql",
                        f.labels_as_of,
                        &f.split,
                        &f.tiers,
                        &f.methods,
                        batch,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_file_as!(
                        LabelSetItem,
                        "src/store/persistence/eval_runs/label_set.sql",
                        f.labels_as_of,
                        &f.split,
                        &f.tiers,
                        &f.methods,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            })
        })
        .await
    }
}
