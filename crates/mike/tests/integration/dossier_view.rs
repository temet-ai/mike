#[path = "dossier_view/api.rs"]
mod api;
#[path = "dossier_view/as_of.rs"]
mod as_of;
#[path = "dossier_view/audiences.rs"]
mod audiences;
#[path = "dossier_view/sections.rs"]
mod sections;
#[path = "dossier_view/verdicts.rs"]
mod verdicts;

use chrono::{DateTime, Duration, Utc};

use crate::turn_facts::Fixture;
use crate::turn_facts::resolution::{build, message};

pub(crate) async fn seeded(f: &Fixture) -> anyhow::Result<(i64, i64, i64)> {
    let first = message(
        f,
        9100,
        0,
        "Refactor the store. Then commit when green.",
        Some(100),
    )
    .await?;
    let second = message(f, 9101, 100, "and add the docs, please push it", Some(200)).await?;
    let third = message(f, 9102, 200, "thanks", Some(300)).await?;
    build(f).await?;
    Ok((first, second, third))
}

pub(crate) fn after(f: &Fixture, secs: i64) -> DateTime<Utc> {
    f.base + Duration::seconds(secs)
}
