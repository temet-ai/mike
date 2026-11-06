use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};

use super::DossierRenderArgs;
use crate::config::{CliOverrides, load};
use crate::dossier::render::render_inputs;
use crate::dossier::view::full::full_view;
use crate::dossier::view::parse::{AudienceArgs, parse_as_of, parse_audience_parts};
use crate::dossier::view::types::Needs;
use crate::store::persistence::PersistHandle;

const DEFAULT_BUDGET: usize = 24_000;

async fn connect(cli: CliOverrides) -> Result<PersistHandle> {
    let config = load::load(cli).context("failed to resolve config for dossier-render")?;
    let Some(database_url) = config.database_url.as_deref() else {
        bail!("dossier-render requires a database_url (set it in /settings or MIKE_DATABASE_URL)");
    };
    PersistHandle::connect_read_only(database_url)
        .await
        .context("dossier-render: could not connect to the database")
}

async fn session_db_id(persist: &PersistHandle, session: &str) -> Result<i64> {
    if let Ok(id) = session.parse::<i64>() {
        return Ok(id);
    }
    persist
        .resolve_session_db_id(session)
        .await?
        .with_context(|| format!("no session {session}"))
}

fn parse_at(value: Option<&str>) -> Result<DateTime<Utc>> {
    match value {
        Some(text) => Ok(DateTime::parse_from_rfc3339(text)
            .with_context(|| format!("--as-of {text:?} is not RFC 3339"))?
            .with_timezone(&Utc)),
        None => Ok(Utc::now()),
    }
}

pub(super) async fn dossier_render(args: DossierRenderArgs, cli: CliOverrides) -> Result<()> {
    let persist = connect(cli).await?;
    if let Some(path) = &args.e_parity {
        return super::dossier_render_parity::run(&persist, path, &args).await;
    }
    let Some(session) = args.session.as_deref() else {
        bail!("--session is required without --e-parity");
    };
    let session = session_db_id(&persist, session).await?;
    let as_of = parse_as_of(parse_at(args.as_of.as_deref())?, Some(&args.mode))
        .map_err(anyhow::Error::msg)?;
    let audience = parse_audience_parts(&AudienceArgs {
        audience: &args.audience,
        turn_id: args.turn,
        slices: args.slices.as_deref(),
        sections: args.sections.as_deref(),
        budget_chars: args.budget_chars.unwrap_or(DEFAULT_BUDGET),
    })
    .map_err(anyhow::Error::msg)?;
    let needs = Needs::for_inspection(audience.as_ref().map(|(audience, _)| audience));
    let inputs = crate::dossier::view::load(&persist, session, as_of, needs).await?;
    let rendered = match &audience {
        Some((audience, budget)) => Some(render_inputs(&inputs, audience, *budget)?),
        None => None,
    };
    let view = (args.full_view || rendered.is_none()).then(|| full_view(&inputs));
    let out = serde_json::json!({
        "session_db_id": session,
        "as_of": as_of,
        "view": view,
        "rendered": rendered,
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
