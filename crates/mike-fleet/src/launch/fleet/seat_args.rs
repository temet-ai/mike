use super::bundle;
use super::definition::FleetDefinition;
use super::environment::contract_path;
use super::plan::SeatPlan;
use super::seat_settings;

pub fn claude_args(def: &FleetDefinition, seat: &SeatPlan) -> Vec<String> {
    let mut args = vec!["--model".to_string(), seat.model.clone()];
    if let Some(level) = effort(seat) {
        args.push("--effort".to_string());
        args.push(level);
    }
    if !seat.agent.trim().is_empty() {
        args.push("--agent".to_string());
        args.push(seat.agent.clone());
    }
    args.push("--settings".to_string());
    args.push(seat_settings::for_seat(def, &seat.seat).to_string());
    if let Some(file) = seat.contract.as_deref().filter(|f| !f.trim().is_empty()) {
        args.push("--append-system-prompt-file".to_string());
        let state = bundle::default_state_dir(&bundle::home());
        let path = bundle::resolve_contract(&def.repo, &state, file)
            .unwrap_or_else(|| contract_path(&def.repo, file));
        args.push(path.to_string_lossy().into_owned());
    }
    args
}

fn effort(seat: &SeatPlan) -> Option<String> {
    let named = seat
        .effort
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty());
    match named {
        Some(level) => Some(level.to_string()),
        None if seat.model.contains("haiku") => Some("medium".to_string()),
        None => None,
    }
}
