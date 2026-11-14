use anyhow::{Context, Result, bail};

use crate::analysis::judge::slices::{JudgeSlice, SliceTexts, render_turnfacts};
use crate::dossier::render::judge::{JudgeSectionId, JudgeView};
use crate::dossier::render::{TypedView, render};
use crate::dossier::view::types::{AsOf, Audience, Budget};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::dossier_repo::DossierPersist as _;
use crate::store::persistence::turn_verdicts::JudgeTurnContextRow;

const DOSSIER_SLICES: [JudgeSlice; 3] = [
    JudgeSlice::Asks,
    JudgeSlice::Directives,
    JudgeSlice::Corrections,
];

fn section_body(view: &JudgeView, id: JudgeSectionId) -> Option<String> {
    view.sections
        .iter()
        .find(|section| section.id == id)
        .map(|section| section.body.clone())
}

async fn dossier_view(persist: &PersistHandle, row: &JudgeTurnContextRow) -> Result<JudgeView> {
    let rendered = render(
        persist,
        row.session_id,
        AsOf::reconstructed_for_eval(row.closed_at),
        &Audience::Judge {
            turn_id: row.turn_id,
        },
        Budget::JudgeSlices,
    )
    .await
    .with_context(|| format!("dossier for turn {} at its close", row.turn_id))?;
    match rendered.typed {
        TypedView::Judge(view) => Ok(*view),
        _ => bail!("dossier for turn {} is not a judge view", row.turn_id),
    }
}

async fn turnfacts_text(persist: &PersistHandle, row: &JudgeTurnContextRow) -> Result<String> {
    let facts = persist
        .dossier()
        .fetch_turn_facts_by_id(row.turn_id)
        .await
        .with_context(|| format!("turn facts for turn {}", row.turn_id))?;
    Ok(render_turnfacts(facts.as_ref()))
}

pub async fn load_slice_texts(
    persist: &PersistHandle,
    row: &JudgeTurnContextRow,
    slices: &[JudgeSlice],
) -> Result<SliceTexts> {
    let mut texts = SliceTexts::default();
    if DOSSIER_SLICES.iter().any(|s| slices.contains(s)) {
        let view = dossier_view(persist, row).await?;
        texts.asks = section_body(&view, JudgeSectionId::EarlierOperatorMessages);
        texts.directives = section_body(&view, JudgeSectionId::Directives);
        texts.corrections = section_body(&view, JudgeSectionId::PriorCorrections);
    }
    if slices.contains(&JudgeSlice::Turnfacts) {
        texts.turnfacts = Some(turnfacts_text(persist, row).await?);
    }
    Ok(texts)
}
