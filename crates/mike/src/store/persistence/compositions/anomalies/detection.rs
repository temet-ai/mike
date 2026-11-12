use chrono::{DateTime, Utc};

use super::super::super::{PersistError, PersistHandle};
use super::types::{AnomalyClassification, AnomalyGroup, AnomalyObservation, AnomalyRow};

fn anomaly_group_from_row(row: AnomalyRow) -> Result<AnomalyGroup, PersistError> {
    let classification_str = row.classification.ok_or_else(|| {
        PersistError::DataInvariant("anomalies query: NULL classification".to_string())
    })?;
    let prompt_sha256 = row.prompt_sha256;
    let unknown_blocks = row.unknown_blocks;
    let block_text_raw = row.block_text;
    let count = row.occurrence_count.ok_or_else(|| {
        PersistError::DataInvariant("anomalies query: NULL occurrence_count".to_string())
    })?;
    let first_seen = row.first_seen.ok_or_else(|| {
        PersistError::DataInvariant("anomalies query: NULL first_seen".to_string())
    })?;
    let last_seen = row.last_seen.ok_or_else(|| {
        PersistError::DataInvariant("anomalies query: NULL last_seen".to_string())
    })?;
    let observations = row.observations.0;
    let firing_rule_ids = row.group_firing_rule_ids.unwrap_or_default();

    let classification = match classification_str.as_str() {
        "unknown-class" => AnomalyClassification::UnknownClass,
        "structural-mismatch" => AnomalyClassification::StructuralMismatch,
        other => {
            return Err(PersistError::DataInvariant(format!(
                "anomalies query produced unknown classification {other:?}"
            )));
        }
    };

    let block_text = match classification {
        AnomalyClassification::UnknownClass => block_text_raw.filter(|s| !s.is_empty()),
        AnomalyClassification::StructuralMismatch => None,
    };

    let signature_hash = match classification {
        AnomalyClassification::UnknownClass => prompt_sha256.clone().ok_or_else(|| {
            PersistError::DataInvariant(
                "anomalies query: unknown-class group with NULL prompt_sha256".to_string(),
            )
        })?,
        AnomalyClassification::StructuralMismatch => {
            let value = unknown_blocks.as_ref().ok_or_else(|| {
                PersistError::DataInvariant(
                    "anomalies query: structural-mismatch group with NULL unknown_blocks"
                        .to_string(),
                )
            })?;
            crate::wire::prompt_blocks::signature_hash_for_unknown_blocks(value)
        }
    };

    let persona = block_text
        .as_deref()
        .and_then(crate::extract::request::extract_subagent_persona);

    let sub_pattern = crate::compose_prompt::sub_patterns::recognise(
        classification,
        unknown_blocks.as_ref(),
        &firing_rule_ids,
        persona.as_deref(),
    );

    Ok(AnomalyGroup {
        classification,
        signature_hash,
        prompt_sha256,
        block_text,
        unknown_blocks,
        count,
        first_seen,
        last_seen,
        observations,
        sub_pattern,
        firing_rule_ids,
        persona,
    })
}

impl PersistHandle {
    pub async fn list_anomalies_windowed(
        &self,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AnomalyGroup>, PersistError> {
        crate::metrics::time_db("list_anomalies", async {
            let rows = sqlx::query_file_as!(
                AnomalyRow,
                "src/store/persistence/compositions/anomalies/anomaly_groups.sql",
                date_from,
                date_to,
                limit,
                offset,
            )
            .fetch_all(&self.pool)
            .await?;

            let mut out: Vec<AnomalyGroup> = Vec::with_capacity(rows.len());
            for row in rows {
                out.push(anomaly_group_from_row(row)?);
            }
            Ok(out)
        })
        .await
    }
}
