use std::time::Duration;

use super::state::Commit;

struct Git<'a>(&'a str);

impl Git<'_> {
    async fn run(&self, args: &[&str]) -> std::io::Result<Vec<u8>> {
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(self.0)
            .args(args)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output()
            .await?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(std::io::Error::other(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }
}

const READ_TIMEOUT: Duration = Duration::from_secs(5);
const FRESH_SECS: i64 = 900;

pub fn is_commit_command(tool_name: &str, command: &str) -> bool {
    tool_name == "Bash" && command.contains("git commit")
}

pub fn parse_head(raw: &str) -> Option<(String, String, i64)> {
    let mut parts = raw.splitn(3, '\u{0}');
    let sha = parts.next()?.trim().to_owned();
    let committed_at = parts.next()?.trim().parse::<i64>().ok()?;
    let message = parts.next()?.trim().to_owned();
    (!sha.is_empty() && !message.is_empty()).then_some((sha, message, committed_at))
}

pub fn belongs_to_command(command: &str, message: &str, committed_at: i64, now: i64) -> bool {
    let subject = message.lines().next().unwrap_or_default().trim();
    !subject.is_empty() && command.contains(subject) && now - committed_at <= FRESH_SECS
}

async fn text(git: &Git<'_>, args: &[&str]) -> Option<String> {
    let bytes = tokio::time::timeout(READ_TIMEOUT, git.run(args))
        .await
        .ok()?
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub async fn read(checkout: &str, command: &str, now: i64) -> Option<Commit> {
    let git = Git(checkout);
    let head = text(&git, &["log", "-1", "--format=%H%x00%ct%x00%B"]).await?;
    let (sha, message, committed_at) = parse_head(&head)?;
    if !belongs_to_command(command, &message, committed_at, now) {
        return None;
    }
    let files = text(&git, &["show", "--format=", "--name-only", &sha]).await?;
    let diff = text(
        &git,
        &["show", "--format=", "--no-color", "--unified=1", &sha],
    )
    .await?;
    Some(Commit {
        subject: message.lines().next().unwrap_or_default().trim().to_owned(),
        files: files.lines().map(str::to_owned).collect(),
        sha,
        message,
        diff,
    })
}
