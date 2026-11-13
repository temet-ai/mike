use crate::store::persistence::PersistHandle;
use crate::store::persistence::dossier_entries::{
    DIRECTIVES_VERSION, DossierEntryRow, PROMISES_VERSION,
};
use crate::store::persistence::dossier_repo::DossierPersist as _;
use crate::store::persistence::dossier_view::{GoalsRow, OpenTextRow};
use crate::store::persistence::steerer_repo::SteererPersist as _;
use crate::store::persistence::steering::SessionNote;

use super::types::{AsOf, AsOfMode, DossierError, DossierInputs, Exclusion, Needs};

pub fn current_extractor_versions() -> Vec<String> {
    vec![DIRECTIVES_VERSION.to_owned(), PROMISES_VERSION.to_owned()]
}

async fn entries(
    persist: &PersistHandle,
    session_db_id: i64,
    as_of: AsOf,
    wanted: bool,
) -> Result<Vec<DossierEntryRow>, DossierError> {
    if !wanted {
        return Ok(Vec::new());
    }
    let rows = match as_of.mode {
        AsOfMode::Knowledge => {
            persist
                .dossier()
                .list_entries_as_of(session_db_id, as_of.at)
                .await?
        }
        AsOfMode::Reconstructed => {
            persist
                .dossier()
                .list_entries_reconstructed(session_db_id, as_of.at, &current_extractor_versions())
                .await?
        }
    };
    Ok(rows)
}

async fn goals(
    persist: &PersistHandle,
    session_db_id: i64,
    as_of: AsOf,
    wanted: bool,
) -> Result<Option<GoalsRow>, DossierError> {
    if !wanted {
        return Ok(None);
    }
    let row = persist
        .dossier()
        .fetch_session_goals_row(session_db_id)
        .await?;
    Ok(row.filter(|g| g.updated_at <= as_of.at))
}

async fn open_turn(
    persist: &PersistHandle,
    as_of: AsOf,
    turn: Option<i64>,
) -> Result<Option<(i64, Vec<OpenTextRow>)>, DossierError> {
    let Some(turn_id) = turn else {
        return Ok(None);
    };
    let rows = persist
        .dossier()
        .fetch_open_turn_text(turn_id, as_of.at)
        .await?;
    Ok(Some((turn_id, rows)))
}

const NOTES_READ: i64 = 20;

async fn notes_oldest_first(
    persist: &PersistHandle,
    session_db_id: i64,
    as_of: AsOf,
    window: Option<chrono::Duration>,
) -> Result<Vec<SessionNote>, DossierError> {
    let Some(window) = window else {
        return Ok(Vec::new());
    };
    let mut notes = persist
        .steerer()
        .session_notes_since(session_db_id, as_of.at - window, as_of.at, NOTES_READ)
        .await?;
    notes.reverse();
    Ok(notes)
}

async fn semantic_sections(
    persist: &PersistHandle,
    turn_id: i64,
    as_of: AsOf,
) -> Option<crate::semantic::sections::SemanticSections> {
    match crate::semantic::sections::steerer_sections(persist, turn_id, as_of).await {
        Ok(sections) => Some(sections),
        Err(e) => {
            tracing::warn!(turn_id, error = %e, "dossier: semantic sections unavailable");
            None
        }
    }
}

fn standing_exclusions() -> Vec<Exclusion> {
    let mut out = Vec::new();
    for section in ["similar_corrections", "similar_exchanges"] {
        out.push(Exclusion {
            section,
            reason:
                "judge slice not approved (D1); the steerer audience reads it from semantic context"
                    .to_owned(),
            count: 0,
        });
    }
    out
}

async fn attach_live_verdicts(
    persist: &PersistHandle,
    inputs: &mut DossierInputs,
) -> Result<(), DossierError> {
    let reconstructed = matches!(inputs.as_of.mode, AsOfMode::Reconstructed);
    let rows = persist
        .dossier()
        .fetch_live_verdicts_as_of(inputs.session.session_db_id, inputs.as_of.at, reconstructed)
        .await?;
    inputs.reactions = super::corrections::reactions_from_verdicts(rows);
    let mut from_verdicts = super::corrections::corrections_from_verdicts(inputs);
    inputs.corrections.append(&mut from_verdicts);
    inputs
        .corrections
        .sort_by_key(|c| (c.correcting_at, c.correcting_request_id));
    Ok(())
}

pub async fn load(
    persist: &PersistHandle,
    session_db_id: i64,
    as_of: AsOf,
    needs: Needs,
) -> Result<DossierInputs, DossierError> {
    let session = persist
        .dossier()
        .fetch_session_header(session_db_id)
        .await?
        .ok_or(DossierError::UnknownSession(session_db_id))?;
    let stream_fut = async {
        if needs.stream {
            Ok::<_, DossierError>(
                persist
                    .dossier()
                    .fetch_stream_as_of(session_db_id, as_of.at)
                    .await?,
            )
        } else {
            Ok(Vec::new())
        }
    };
    let turns_fut = async {
        if needs.turns {
            Ok::<_, DossierError>(
                persist
                    .dossier()
                    .fetch_turns_as_of(session_db_id, as_of.at)
                    .await?,
            )
        } else {
            Ok(Vec::new())
        }
    };
    let ledger_fut = async {
        if needs.ledger {
            Ok::<_, DossierError>(
                persist
                    .dossier()
                    .fetch_ledger_as_of(session_db_id, as_of.at)
                    .await?,
            )
        } else {
            Ok(Vec::new())
        }
    };
    let (stream, turns, ledger, entries, goals, open_turn, notes) = tokio::try_join!(
        stream_fut,
        turns_fut,
        ledger_fut,
        entries(persist, session_db_id, as_of, needs.entries),
        goals(persist, session_db_id, as_of, needs.goals),
        open_turn(persist, as_of, needs.open_turn),
        notes_oldest_first(persist, session_db_id, as_of, needs.notes_window),
    )?;
    let mut inputs = DossierInputs {
        session,
        as_of,
        stream,
        turns,
        ledger,
        entries,
        goals,
        open_turn,
        corrections: Vec::new(),
        reactions: Vec::new(),
        exclusions: standing_exclusions(),
        steering_facts: None,
        semantic: None,
        notes,
    };
    if needs.labels {
        inputs.corrections = super::corrections::load_label_corrections(persist, &inputs).await?;
    }
    if needs.verdicts {
        attach_live_verdicts(persist, &mut inputs).await?;
    }
    if let Some(turn_id) = needs.semantic {
        inputs.semantic = semantic_sections(persist, turn_id, as_of).await;
    }
    if let Some(turn_id) = needs.steering_facts {
        inputs.steering_facts = Some(
            crate::dossier::steering_facts::SteeringFacts::as_of(
                persist,
                session_db_id,
                turn_id,
                as_of.at,
            )
            .await?,
        );
    }
    Ok(inputs)
}
