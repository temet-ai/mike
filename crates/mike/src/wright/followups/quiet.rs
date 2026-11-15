use std::path::Path;
use std::time::Duration;

use super::context::{Here, Place, Target};
use super::runner::{Cmd, CommandRunner, output};
use crate::wright::investigate::admission::{Locks, load_over, machine_busy};

const REMOTE_PROBE: &str = "cat /proc/loadavg; nproc; if flock -n /run/user/$(id -u)/mike-heavy-build.lock true; then echo lock-free; else echo lock-held; fi";
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) fn parse_probe(stdout: &str) -> Result<(), String> {
    let mut lines = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let (Some(loadavg), Some(nproc), Some(lock)) = (lines.next(), lines.next(), lines.next())
    else {
        return Err(format!(
            "the probe printed {stdout:?}, not load, processors and lock"
        ));
    };
    let processors: usize = nproc
        .parse()
        .map_err(|e| format!("the probe's processor count {nproc:?} is not a number: {e}"))?;
    if let Some(reason) = load_over(loadavg, processors) {
        return Err(reason);
    }
    match lock {
        "lock-free" => Ok(()),
        "lock-held" => Err("a heavy build holds mike-heavy-build.lock".to_owned()),
        other => Err(format!("the probe's lock line is {other:?}")),
    }
}

async fn remote<R: CommandRunner>(runner: &R, host: &str) -> Result<(), String> {
    let cmd = Cmd::remote_line(host, REMOTE_PROBE).with_timeout(PROBE_TIMEOUT);
    let out = output(runner, &cmd).await.map_err(|e| e.to_string())?;
    if !out.ok() {
        return Err(format!(
            "`{}` exited {:?}: {}",
            cmd.line(),
            out.code,
            out.combined()
        ));
    }
    parse_probe(&out.stdout)
}

fn local(here: &Here, folder: &str) -> Result<(), String> {
    let deploy = Path::new(folder).join(".deploy").join("deploy.lock");
    let locks = Locks {
        loadavg: &here.loadavg,
        deploy: &deploy,
        build: &here.build_lock,
    };
    machine_busy(&locks).map_or(Ok(()), Err)
}

pub(crate) async fn quiet<R: CommandRunner>(
    runner: &R,
    here: &Here,
    target: &Target,
) -> Result<(), String> {
    match &target.place {
        Place::Local => local(here, &target.folder),
        Place::Remote(host) => remote(runner, host).await,
    }
}
