use std::path::PathBuf;

use super::browser_guard;

pub(crate) struct Setup {
    pub directory: PathBuf,
    pub command: String,
}

pub(crate) fn setup(arguments: &[String]) -> Result<Option<Setup>, String> {
    Ok(browser_guard::directory(arguments)
        .zip(browser_guard::shell_setup(arguments))
        .map(|(directory, command)| Setup { directory, command }))
}

pub(crate) fn terminal_command(program: &str, arguments: &[String]) -> Result<Vec<String>, String> {
    Ok(browser_guard::terminal_command(program, arguments))
}
