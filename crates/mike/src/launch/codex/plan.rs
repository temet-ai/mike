use std::path::Path;

use super::contract::{SeatKind, render_contract};
use super::fleet::{SeatLaunch, begin, identity};
use super::options::{McpBinding, SeatOptions, seat_codex_home};
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::plan::SeatPlan;
use crate::launch::fleet::{bundle, environment, report_proof, seat_settings};
use crate::store::persistence::PersistHandle;

pub async fn begin_planned(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run_id: &str,
    plan: &SeatPlan,
    scratch: &Path,
    name: &str,
) -> Result<(), String> {
    let options = seat_options(def, run_id, plan, scratch)?;
    let launch = SeatLaunch {
        run_id,
        seat: &plan.seat,
        session: plan.session,
        prompt: report_proof::starting_prompt(persist, def, run_id, plan).await,
        identity: identity(name, &plan.role, &plan.instructions, &plan.goal),
    };
    begin(persist, options, launch).await
}

pub fn seat_options(
    def: &FleetDefinition,
    run_id: &str,
    plan: &SeatPlan,
    scratch: &Path,
) -> Result<SeatOptions, String> {
    let contract = contract_text(def, plan)?;
    let profile = seat_settings::profile_for(&plan.seat);
    let kind = SeatKind::from_profile(profile);
    Ok(SeatOptions {
        program: "codex".into(),
        cwd: def.repo.clone().into(),
        codex_home: seat_codex_home(scratch, run_id),
        model: Some(plan.model.clone()).filter(|m| !m.trim().is_empty()),
        effort: plan.effort.clone(),
        developer_instructions: render_contract(kind, &plan.seat, plan.session, &contract),
        mcp: Some(McpBinding {
            command: "mike-mcp".into(),
            env: vec![
                (seat_settings::PROFILE_ENV.into(), profile.into()),
                ("CLAUDE_CODE_SESSION_ID".into(), plan.session.to_string()),
            ],
        }),
    })
}

fn contract_text(def: &FleetDefinition, plan: &SeatPlan) -> Result<String, String> {
    let Some(file) = plan.contract.as_deref().filter(|f| !f.trim().is_empty()) else {
        return Ok(String::new());
    };
    let state = bundle::default_state_dir(&bundle::home());
    let path = bundle::resolve_contract(&def.repo, &state, file)
        .unwrap_or_else(|| environment::contract_path(&def.repo, file));
    std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "could not read contract {} for seat {}: {error}",
            path.display(),
            plan.seat
        )
    })
}
