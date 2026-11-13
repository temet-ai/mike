use mike::store::persistence::dossier_repo::DossierPersist as _;
#[path = "dossier_entries/as_of.rs"]
mod as_of;
#[path = "dossier_entries/directives.rs"]
mod directives;
#[path = "dossier_entries/promises.rs"]
mod promises;

use chrono::{DateTime, Utc};
use mike::store::persistence::dossier_entries::DossierEntryRow;

use crate::turn_facts::Fixture;

pub(crate) async fn entries(f: &Fixture) -> anyhow::Result<Vec<DossierEntryRow>> {
    Ok(f.handle
        .dossier()
        .list_entries_as_of(f.session_id, far_future())
        .await?)
}

pub(crate) fn far_future() -> DateTime<Utc> {
    Utc::now() + chrono::Duration::days(3650)
}
