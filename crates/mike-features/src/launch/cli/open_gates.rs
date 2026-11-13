use std::path::PathBuf;

use super::open::{OpenFailure, fail};
use super::runner::Runner;
use crate::launch::worktree::CarryOverOutcome;
use crate::launch::{LaunchError, LaunchOutput, SplitLaunchArgs, TrustOutcome};

pub fn report_carry_over(outcomes: &[CarryOverOutcome]) {
    for outcome in outcomes {
        match outcome {
            CarryOverOutcome::Linked { relative } => {
                eprintln!("mike spawn: worktree: linked {}", relative.display());
            }
            CarryOverOutcome::NoSource { relative } => {
                eprintln!(
                    "mike spawn: worktree: {} does not exist in the main checkout; nothing to carry over",
                    relative.display()
                );
            }
            CarryOverOutcome::AlreadyPresent { relative } => {
                eprintln!(
                    "mike spawn: worktree: {} already exists in the worktree; not overwritten",
                    relative.display()
                );
            }
            CarryOverOutcome::Failed { relative, detail } => {
                eprintln!(
                    "mike spawn: worktree: could not link {}: {detail}",
                    relative.display()
                );
            }
        }
    }
}

pub async fn trust_gate(
    runner: &Runner<'_>,
    folder: &std::path::Path,
    split: &SplitLaunchArgs,
    no_trust: bool,
    output: &LaunchOutput,
) -> Result<PathBuf, OpenFailure> {
    let write = split.trust || (runner.config.auto_trust && !no_trust);
    match runner.trust.ensure(folder, write).await {
        Ok(TrustOutcome::Trusted { target }) => Ok(target),
        Ok(TrustOutcome::Written { target }) => {
            eprintln!("mike spawn: recorded {} as trusted", target.display());
            Ok(target)
        }
        Ok(TrustOutcome::Unknown { target, detail }) => {
            eprintln!(
                "mike spawn: trust state unknown ({detail}); the trust dialog may swallow the prompt"
            );
            Ok(target)
        }
        Ok(TrustOutcome::Untrusted { target, remedy }) => Err(fail(
            output.clone(),
            LaunchError::Refused(format!(
                "{} is not a trusted folder and the launch would lose its prompt to the trust \
                 dialog; {remedy}",
                target.display()
            )),
            "untrusted_folder",
        )),
        Err(e) => Err(fail(output.clone(), e, "trust_gate")),
    }
}

pub async fn surface_cap(runner: &Runner<'_>, output: &LaunchOutput) -> Result<(), OpenFailure> {
    let (warn, max) = (runner.config.warn_surfaces, runner.config.max_surfaces);
    if warn == 0 && max == 0 {
        return Ok(());
    }
    let count = match runner.backend.list().await {
        Ok(surfaces) => surfaces.len(),
        Err(e) => {
            eprintln!("mike spawn: could not count surfaces ({e}); the cap is not enforced");
            return Ok(());
        }
    };
    if max > 0 && count >= max as usize {
        return Err(fail(
            output.clone(),
            LaunchError::Refused(format!(
                "{count} surfaces are open and launch_max_surfaces is {max}; close some \
                 (`mike spawn list`, `mike spawn kill --session <uuid>`) or raise the cap"
            )),
            "surface_cap",
        ));
    }
    if warn > 0 && count >= warn as usize {
        eprintln!("mike spawn: {count} surfaces are open (warn at {warn}, refuse at {max})");
    }
    Ok(())
}
