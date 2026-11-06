use std::time::Duration;

use clap::Args;
use serde_json::json;
use uuid::Uuid;

use super::api::{Api, FleetError};
use super::commit::{own_fleet, own_session, resolve};
use crate::launch::fleet::gate::{DEFAULT_WAIT_SECONDS, GateReport};
use crate::launch::fleet::gate_text;

const ANSWER_MARGIN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Args)]
pub struct GateArgs {
    #[arg(
        long,
        help = "The fleet's uuid, slug or run id; default: the MIKE_RUN_ID of this pane"
    )]
    pub fleet: Option<String>,
    #[arg(
        long,
        help = "The seat's session id; default: the MIKE_SESSION_ID of this pane"
    )]
    pub session: Option<Uuid>,
    #[arg(
        long,
        help = "Only say whether the gate has a verdict for the branch's newest commit or is running now; starts nothing"
    )]
    pub status: bool,
    #[arg(
        long,
        value_name = "SECONDS",
        default_value_t = DEFAULT_WAIT_SECONDS,
        help = "How long to wait for a gate that is still running before answering that it is; mike mails the verdict when it ends"
    )]
    pub wait: u64,
}

pub(super) async fn gate_text(api: &Api, args: &GateArgs) -> Result<String, FleetError> {
    let session = own_session(args.session)?;
    let fleet = resolve(api, &own_fleet(args.fleet.as_deref())?).await?;
    let body = json!({"session": session, "status": args.status, "wait_seconds": args.wait});
    let patience = Duration::from_secs(args.wait) + ANSWER_MARGIN;
    let answer = api
        .post_json_and_wait(api.url(&[&fleet, "gate"])?, &body, patience)
        .await?;
    let report: GateReport = api.parse(&answer)?;
    Ok(gate_text::text(&report))
}
