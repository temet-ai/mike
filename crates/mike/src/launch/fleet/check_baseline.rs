use crate::launch::fleet::check::CheckRequest;
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::diagnostics::Diagnostic;
use crate::launch::fleet::gate_slot::check_slot_for;
use crate::store::persistence::fleet::TreeCheckRow;

const MARK: &str = "#baseline:";

pub(in crate::launch::fleet) fn is_baseline(run: &str) -> bool {
    run.contains(MARK)
}

pub(in crate::launch::fleet) fn base_run(run: &str) -> &str {
    run.split_once(MARK).map_or(run, |(base, _)| base)
}

pub(in crate::launch::fleet) fn state_after(_run: &str, state: &str, code: Option<i32>) -> String {
    if code.is_none() {
        format!("{state}:unverified")
    } else {
        state.to_string()
    }
}

pub(in crate::launch::fleet) async fn request(
    def: &FleetDefinition,
    run: &str,
    request: &CheckRequest,
) -> Result<(CheckRequest, String), String> {
    let mut request = request.clone();
    if request
        .commit
        .as_deref()
        .is_none_or(|c| c.trim().is_empty())
    {
        return Ok((request, run.to_string()));
    }
    let at = super::target(def, request.commit.as_deref()).await?;
    use sha2::{Digest, Sha256};
    let command = hex::encode(Sha256::digest(def.check.command.as_bytes()));
    let ledger = format!("{run}{MARK}{}:{command}", at.commit);
    request.commit = Some(at.commit);
    Ok((request, ledger))
}

pub(in crate::launch::fleet) fn diagnostics(
    def: &FleetDefinition,
    row: &TreeCheckRow,
) -> Vec<Diagnostic> {
    let root = diagnostic_root(def, &row.run_id);
    crate::launch::fleet::diagnostics::parse(&row.output, root.as_deref().unwrap_or(&def.repo))
}

pub(in crate::launch::fleet) fn diagnostic_root(
    def: &FleetDefinition,
    run: &str,
) -> Option<String> {
    if super::is_command(run) && !is_baseline(run) {
        return None;
    }
    check_slot_for(&def.repo).map(|path| {
        std::fs::canonicalize(&path)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    })
}
