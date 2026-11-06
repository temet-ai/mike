pub mod allocate;
pub mod cut;
pub mod judge;
pub mod sentences;
pub mod session_judge;
pub mod steerer;
pub mod steerer_facts;
pub mod steerer_notes;

#[cfg(test)]
mod allocate_tests;
#[cfg(test)]
mod render_tests;

use serde::Serialize;

use crate::dossier::view::digest::{SourceVersions, inputs_digest, source_versions};
use crate::dossier::view::types::{
    AsOf, Audience, AudienceCode, Budget, DossierError, DossierInputs, Needs,
};
use crate::store::persistence::PersistHandle;

pub const RENDERER_VERSION: &str = "dossier-r1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SectionReport {
    pub id: String,
    pub items_total: u32,
    pub items_full: u32,
    pub items_compressed: u32,
    pub items_in_spans: u32,
    pub chars: u32,
}

impl SectionReport {
    pub fn whole(id: &str, items: usize, chars: usize) -> Self {
        let items = u32::try_from(items).unwrap_or(u32::MAX);
        Self {
            id: id.to_owned(),
            items_total: items,
            items_full: items,
            items_compressed: 0,
            items_in_spans: 0,
            chars: u32::try_from(chars).unwrap_or(u32::MAX),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "audience", rename_all = "snake_case")]
pub enum TypedView {
    Judge(Box<judge::JudgeView>),
    SessionJudge(Box<session_judge::SessionJudgeView>),
    Steerer(Box<steerer::SteererView>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderedDossier {
    pub text: String,
    pub renderer_version: &'static str,
    pub audience: AudienceCode,
    pub as_of: AsOf,
    pub budget_chars: Option<usize>,
    pub sections: Vec<SectionReport>,
    pub typed: TypedView,
    pub inputs_digest: String,
    pub source_versions: SourceVersions,
}

impl RenderedDossier {
    pub fn digest_version(&self) -> String {
        let budget = self
            .budget_chars
            .map_or_else(|| "slices".to_owned(), |n| n.to_string());
        format!(
            "{}/{}/{budget}",
            self.renderer_version,
            self.audience.code()
        )
    }
}

pub fn render_inputs(
    inputs: &DossierInputs,
    audience: &Audience,
    budget: Budget,
) -> Result<RenderedDossier, DossierError> {
    let code = audience.code();
    let (text, sections, typed, budget_chars) = match (audience, budget) {
        (Audience::Judge { turn_id }, Budget::JudgeSlices) => {
            let rendered = judge::render_judge(inputs, *turn_id)?;
            (
                rendered.text,
                rendered.reports,
                TypedView::Judge(Box::new(rendered.view)),
                None,
            )
        }
        (Audience::SessionJudge { slices }, Budget::Chars(n)) => {
            let rendered = session_judge::render_session_judge(inputs, *slices, n);
            (
                rendered.text,
                rendered.reports,
                TypedView::SessionJudge(Box::new(rendered.view)),
                Some(n),
            )
        }
        (Audience::Steerer { turn_id, sections }, Budget::Chars(n)) => {
            let rendered = steerer::render_steerer(inputs, *turn_id, sections, n);
            (
                rendered.allocated.text,
                rendered.allocated.reports,
                TypedView::Steerer(Box::new(rendered.view)),
                Some(n),
            )
        }
        _ => return Err(DossierError::BudgetMismatch { audience: code }),
    };
    Ok(RenderedDossier {
        text,
        renderer_version: RENDERER_VERSION,
        audience: code,
        as_of: inputs.as_of,
        budget_chars,
        sections,
        typed,
        inputs_digest: inputs_digest(inputs),
        source_versions: source_versions(inputs),
    })
}

pub async fn render(
    persist: &PersistHandle,
    session_db_id: i64,
    as_of: AsOf,
    audience: &Audience,
    budget: Budget,
) -> Result<RenderedDossier, DossierError> {
    let inputs =
        crate::dossier::view::load(persist, session_db_id, as_of, Needs::for_audience(audience))
            .await?;
    render_inputs(&inputs, audience, budget)
}
