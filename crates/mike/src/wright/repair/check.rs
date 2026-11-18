use std::path::Path;
use std::time::Duration;

use super::feedback::tail_of;
use super::policy::ChangedFile;
use super::runner::{Cmd, CommandRunner, Program, output};

const CHECK_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const CHECK_TAIL: usize = 4000;

pub(crate) fn crates_of(files: &[ChangedFile]) -> Vec<String> {
    let mut crates: Vec<String> = files
        .iter()
        .filter(|file| file.path.ends_with(".rs") || file.path.ends_with("Cargo.toml"))
        .filter_map(|file| {
            let rest = file.path.strip_prefix("crates/")?;
            let (name, _) = rest.split_once('/')?;
            Some(name.to_owned())
        })
        .collect();
    crates.sort();
    crates.dedup();
    crates
}

pub(crate) async fn run<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    crates: &[String],
) -> Result<(), String> {
    if crates.is_empty() {
        return Ok(());
    }
    let mut args = vec!["check", "--lib"];
    for name in crates {
        args.extend(["-p", name]);
    }
    let cmd = Cmd::new(Program::Cx, worktree, &args).with_timeout(CHECK_TIMEOUT);
    let out = output(runner, &cmd).await.map_err(|e| e.to_string())?;
    if out.ok() {
        return Ok(());
    }
    Err(format!(
        "`{}` exited {:?}: {}",
        cmd.line(),
        out.code,
        tail_of(&out.combined(), CHECK_TAIL)
    ))
}
