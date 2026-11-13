use super::{Pacing, agent_status, read_screen};
use crate::launch::herdr::cli::HerdrCli;

#[derive(Debug, PartialEq, Eq)]
pub enum Blocked {
    Cleared,
    Stays(String),
}

pub async fn wait_until_clear(cli: &HerdrCli, name: &str, pacing: Pacing) -> Blocked {
    for _ in 0..pacing.block_polls {
        match agent_status(cli, name).await.as_deref() {
            Some("blocked") => tokio::time::sleep(pacing.poll).await,
            Some(_) => return Blocked::Cleared,
            None => break,
        }
    }
    Blocked::Stays(describe(cli, name, pacing).await)
}

async fn describe(cli: &HerdrCli, name: &str, pacing: Pacing) -> String {
    let waited = pacing.block_polls;
    match read_screen(cli, name)
        .await
        .as_deref()
        .and_then(dialog_name)
    {
        Some(dialog) => {
            format!("still blocked after {waited} checks by {dialog}, which needs a person")
        }
        None => format!("still blocked after {waited} checks; the pane could not be read"),
    }
}

pub fn dialog_name(screen: &str) -> Option<String> {
    if screen.contains("Loading development channels") {
        return Some("the development-channels dialog".to_string());
    }
    let lines: Vec<&str> = screen
        .lines()
        .map(str::trim)
        .filter(|l| l.chars().any(char::is_alphanumeric))
        .collect();
    if lines.iter().any(|l| l.to_lowercase().contains("trust")) {
        return Some("the folder trust dialog".to_string());
    }
    let line = lines
        .iter()
        .find(|l| l.ends_with('?'))
        .or_else(|| lines.first())?;
    Some(format!("a dialog reading \"{line}\""))
}
