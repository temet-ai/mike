use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use tempfile::TempDir;

use super::runner::{Cmd, CmdOutput, CommandRunner, ProcessRunner, Program};

pub(super) const BUGGY: &str = "add() { echo $(( $1 - $2 )); }\n";
pub(super) const FIXED: &str = "add() { echo $(( $1 + $2 )); }\n";
const CHECK: &str = ". ./src/calc.sh
[ \"$(add 2 3)\" = 5 ] || { echo 'FAIL src/calc.sh'; exit 1; }
for f in src/?.txt; do
  [ -e \"$f\" ] || continue
  grep -q BROKEN \"$f\" && { echo \"FAIL $f\"; exit 1; }
  grep -q SILENT \"$f\" && { echo 'the gate refused the branch'; exit 1; }
done
exit 0
";
const HOOK: &str = r#"#!/usr/bin/env bash
set -u
here=$(cd "$(dirname "$0")" && pwd)
status=0
while read -r _local sha _remote _remote_sha; do
  work="$here/work-$sha"
  rm -rf "$work" && mkdir -p "$work"
  git archive "$sha" | tar -x -C "$work"
  if (cd "$work" && sh scripts/check.sh); then
    echo "pass $sha" >>"$here/gate.log"
  else
    echo "fail $sha" >>"$here/gate.log"
    status=1
  fi
  rm -rf "$work"
done
exit "$status"
"#;
pub(super) const CHECK_NAME: &str = "request_failures";

pub(super) fn sh(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "git {} in {} failed: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

pub(super) fn write(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)?;
    Ok(())
}

fn local_config(dir: &Path, hooks: &Path) -> anyhow::Result<()> {
    let hooks = hooks.display().to_string();
    for (key, value) in [
        ("user.name", "Scratch Peer"),
        ("user.email", "peer@scratch.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", hooks.as_str()),
    ] {
        sh(dir, &["config", key, value])?;
    }
    Ok(())
}

pub(super) struct Scratch {
    pub(super) _dir: TempDir,
    pub(super) remote: PathBuf,
    pub(super) root: PathBuf,
    pub(super) hooks: PathBuf,
    peer: PathBuf,
    merger: PathBuf,
}

impl Scratch {
    pub(super) fn create() -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        let base = dir.path().to_path_buf();
        let (remote, root, hooks) = (
            base.join("remote.git"),
            base.join("main"),
            base.join("hooks"),
        );
        let empty = base.join("no-hooks");
        std::fs::create_dir_all(&empty)?;
        write(&hooks.join("pre-push"), HOOK)?;
        sh(
            &base,
            &["init", "--quiet", "--bare", "-b", "main", "remote.git"],
        )?;
        sh(&base, &["clone", "--quiet", "remote.git", "main"])?;
        local_config(&root, &hooks)?;
        Command::new("chmod")
            .arg("+x")
            .arg(hooks.join("pre-push"))
            .status()?;
        write(&root.join("src/calc.sh"), BUGGY)?;
        write(&root.join("scripts/check.sh"), CHECK)?;
        write(&root.join("README.md"), "scratch\n")?;
        sh(&root, &["add", "--all"])?;
        sh(
            &root,
            &["commit", "--quiet", "-m", "scratch: a replayable defect"],
        )?;
        let seed_hooks = format!("core.hooksPath={}", empty.display());
        sh(
            &root,
            &["-c", &seed_hooks, "push", "--quiet", "origin", "HEAD:main"],
        )?;
        for name in ["peer", "merger"] {
            sh(&base, &["clone", "--quiet", "remote.git", name])?;
            local_config(&base.join(name), &empty)?;
        }
        Ok(Self {
            peer: base.join("peer"),
            merger: base.join("merger"),
            _dir: dir,
            remote,
            root,
            hooks,
        })
    }

    pub(super) fn remote_main(&self) -> anyhow::Result<String> {
        sh(&self.remote, &["rev-parse", "refs/heads/main"])
    }

    pub(super) fn remote_branch(&self, branch: &str) -> Option<String> {
        sh(
            &self.remote,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
        .ok()
    }

    pub(super) fn on_main(&self, path: &str) -> anyhow::Result<String> {
        sh(&self.remote, &["show", &format!("main:{path}")])
    }

    pub(super) fn gate_log(&self) -> String {
        std::fs::read_to_string(self.hooks.join("gate.log")).unwrap_or_default()
    }

    pub(super) fn defect_replays(&self) -> anyhow::Result<bool> {
        let out = Command::new("sh")
            .arg("scripts/check.sh")
            .current_dir(&self.root)
            .output()?;
        Ok(!out.status.success())
    }

    pub(super) fn move_main(&self, file: &str) -> anyhow::Result<String> {
        sh(
            &self.peer,
            &["pull", "--quiet", "--ff-only", "origin", "main"],
        )?;
        write(&self.peer.join(file), "a peer landed this\n")?;
        sh(&self.peer, &["add", "--all"])?;
        sh(
            &self.peer,
            &["commit", "--quiet", "-m", "peer: unrelated change"],
        )?;
        sh(&self.peer, &["push", "--quiet", "origin", "HEAD:main"])?;
        self.remote_main()
    }

    fn squash(&self, branch: &str, title: &str) -> anyhow::Result<String> {
        sh(
            &self.merger,
            &[
                "fetch",
                "--quiet",
                "origin",
                "+refs/heads/*:refs/remotes/origin/*",
            ],
        )?;
        sh(&self.merger, &["reset", "--quiet", "--hard", "origin/main"])?;
        sh(
            &self.merger,
            &["merge", "--quiet", "--squash", &format!("origin/{branch}")],
        )?;
        sh(&self.merger, &["commit", "--quiet", "-m", title])?;
        sh(&self.merger, &["push", "--quiet", "origin", "HEAD:main"])?;
        sh(&self.merger, &["rev-parse", "HEAD"])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PrState {
    Open,
    Merged(String),
    Closed,
}

#[derive(Debug, Clone)]
pub(super) struct Pr {
    pub(super) url: String,
    pub(super) branch: String,
    pub(super) state: PrState,
}

pub(super) struct ScratchGh {
    pub(super) scratch: Arc<Scratch>,
    pub(super) prs: Mutex<Vec<Pr>>,
    pub(super) commands: Mutex<Vec<Cmd>>,
    pub(super) move_main_on_create: Mutex<u32>,
    pub(super) cx_exit: Mutex<i32>,
}

fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn reply(code: i32, stdout: String) -> io::Result<CmdOutput> {
    Ok(CmdOutput {
        code: Some(code),
        stdout,
        stderr: String::new(),
    })
}

fn other(e: anyhow::Error) -> io::Error {
    io::Error::other(e.to_string())
}

impl ScratchGh {
    pub(super) fn new(scratch: Arc<Scratch>) -> Self {
        Self {
            scratch,
            prs: Mutex::new(Vec::new()),
            commands: Mutex::new(Vec::new()),
            move_main_on_create: Mutex::new(0),
            cx_exit: Mutex::new(0),
        }
    }

    fn cx(&self) -> io::Result<CmdOutput> {
        let code = self.cx_exit.lock().map(|code| *code).unwrap_or(1);
        reply(code, String::new())
    }

    pub(super) fn cx_runs(&self) -> Vec<String> {
        self.programs()
            .into_iter()
            .filter(|line| line.starts_with("scripts/cx"))
            .collect()
    }

    fn lock_prs(&self) -> io::Result<std::sync::MutexGuard<'_, Vec<Pr>>> {
        self.prs.lock().map_err(|e| io::Error::other(e.to_string()))
    }

    pub(super) fn prs(&self) -> Vec<Pr> {
        self.prs.lock().map(|prs| prs.clone()).unwrap_or_default()
    }

    pub(super) fn programs(&self) -> Vec<String> {
        self.commands
            .lock()
            .map(|cmds| cmds.iter().map(Cmd::line).collect())
            .unwrap_or_default()
    }

    fn create(&self, args: &[String]) -> io::Result<CmdOutput> {
        let branch = arg_after(args, "--head").unwrap_or_default().to_owned();
        let moves = self
            .move_main_on_create
            .lock()
            .map(|mut n| {
                let due = *n > 0;
                *n = n.saturating_sub(1);
                due
            })
            .unwrap_or(false);
        if moves {
            self.scratch.move_main("docs/peer.md").map_err(other)?;
        }
        let mut prs = self.lock_prs()?;
        let url = format!("https://scratch.invalid/pull/{}", prs.len() + 1);
        prs.push(Pr {
            url: url.clone(),
            branch,
            state: PrState::Open,
        });
        reply(0, format!("{url}\n"))
    }

    fn merge(&self, args: &[String]) -> io::Result<CmdOutput> {
        let url = args.get(2).cloned().unwrap_or_default();
        let head = arg_after(args, "--match-head-commit")
            .unwrap_or_default()
            .to_owned();
        let Some(pr) = self.prs().into_iter().find(|pr| pr.url == url) else {
            return reply(1, format!("no pull request {url}"));
        };
        if self.scratch.remote_branch(&pr.branch).as_deref() != Some(head.as_str()) {
            return reply(1, "head commit does not match".to_owned());
        }
        let merged = self.scratch.squash(&pr.branch, &url).map_err(other)?;
        let mut prs = self.lock_prs()?;
        for open in prs.iter_mut().filter(|p| p.url == url) {
            open.state = PrState::Merged(merged.clone());
        }
        reply(0, String::new())
    }

    fn view(&self, args: &[String]) -> io::Result<CmdOutput> {
        let url = args.get(2).cloned().unwrap_or_default();
        let oid = self
            .prs()
            .into_iter()
            .find_map(|pr| match pr.state {
                PrState::Merged(sha) if pr.url == url => Some(sha),
                _ => None,
            })
            .unwrap_or_default();
        reply(0, format!("{oid}\n"))
    }

    fn close(&self, args: &[String]) -> io::Result<CmdOutput> {
        let url = args.get(2).cloned().unwrap_or_default();
        let mut prs = self.lock_prs()?;
        for pr in prs.iter_mut().filter(|p| p.url == url) {
            pr.state = PrState::Closed;
        }
        reply(0, String::new())
    }

    fn gh(&self, cmd: &Cmd) -> io::Result<CmdOutput> {
        let verb: Vec<&str> = cmd.args.iter().take(2).map(String::as_str).collect();
        match verb.as_slice() {
            ["pr", "create"] => self.create(&cmd.args),
            ["pr", "merge"] => self.merge(&cmd.args),
            ["pr", "view"] => self.view(&cmd.args),
            ["pr", "close"] => self.close(&cmd.args),
            _ => reply(2, format!("scratch gh does not know `{}`", cmd.line())),
        }
    }
}

impl CommandRunner for ScratchGh {
    async fn run(&self, cmd: &Cmd) -> io::Result<CmdOutput> {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(cmd.clone());
        }
        match cmd.program {
            Program::Git => ProcessRunner.run(cmd).await,
            Program::Gh => self.gh(cmd),
            Program::Cx => self.cx(),
        }
    }
}
