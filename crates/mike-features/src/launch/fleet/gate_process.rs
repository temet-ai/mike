use std::path::Path;
use std::time::Duration;

pub async fn run(
    slot: &Path,
    command: &str,
    run: &str,
    (limit, what): (Duration, &str),
    log_path: Option<&Path>,
    setting: Option<crate::launch::fleet::process_tripwire::Setting>,
) -> (Option<i32>, String) {
    crate::launch::fleet::managed_process::run(crate::launch::fleet::managed_process::Command {
        slot: slot.to_path_buf(),
        text: command.to_string(),
        run: run.to_string(),
        budget: (limit, what.to_string()),
        log: log_path.map(Path::to_path_buf),
        setting,
    })
    .await
}

#[cfg(test)]
#[path = "gate_process_tests.rs"]
pub mod tests;
