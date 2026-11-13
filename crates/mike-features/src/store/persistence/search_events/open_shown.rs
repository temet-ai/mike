use std::collections::BTreeMap;

use super::quoted::{ShownText, WindowTurn, quoted_references};
use crate::store::persistence::PersistError;

pub struct OpenShownRow {
    pub event_id: i64,
    pub reference: Option<String>,
    pub shown_text: Option<String>,
    pub answer: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Default)]
struct EventTexts {
    shown: Vec<ShownText>,
    turns: Vec<WindowTurn>,
}

pub struct QuotedPairs {
    pub event_ids: Vec<i64>,
    pub references: Vec<String>,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn open_shown_rows(
        self,
        batch: i64,
        window_turns: i64,
    ) -> Result<Vec<OpenShownRow>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/search_events/open_shown.sql",
            batch,
            window_turns,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| OpenShownRow {
                event_id: row.event_id,
                reference: row.reference,
                shown_text: row.shown_text,
                answer: row.answer,
                prompt: row.prompt,
            })
            .collect())
    }
}

pub fn quoted_pairs(rows: Vec<OpenShownRow>) -> QuotedPairs {
    let mut events: BTreeMap<i64, EventTexts> = BTreeMap::new();
    for row in rows {
        let texts = events.entry(row.event_id).or_default();
        match (row.reference, row.shown_text, row.answer) {
            (Some(reference), Some(text), _) => texts.shown.push(ShownText { reference, text }),
            (_, _, Some(answer)) => texts.turns.push(WindowTurn {
                answer,
                prompt: row.prompt.unwrap_or_default(),
            }),
            _ => {}
        }
    }
    let mut pairs = QuotedPairs {
        event_ids: Vec::new(),
        references: Vec::new(),
    };
    for (event_id, texts) in events {
        for reference in quoted_references(&texts.shown, &texts.turns) {
            pairs.event_ids.push(event_id);
            pairs.references.push(reference);
        }
    }
    pairs
}
