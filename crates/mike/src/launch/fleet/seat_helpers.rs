use std::path::PathBuf;

use super::{browser_guard, cargo_policy};

pub(crate) struct Setup {
    pub directory: PathBuf,
    pub command: String,
}

pub(crate) fn setup(arguments: &[String]) -> Result<Option<Setup>, String> {
    let browser = browser_guard::directory(arguments).zip(browser_guard::shell_setup(arguments));
    let cargo = cargo_policy::setup(arguments)?;
    let helpers: Vec<_> = browser.into_iter().chain(cargo).collect();
    let Some((directory, _)) = helpers.first() else {
        return Ok(None);
    };
    Ok(Some(Setup {
        directory: directory.clone(),
        command: helpers
            .iter()
            .map(|(_, command)| command.as_str())
            .collect::<Vec<_>>()
            .join(" && "),
    }))
}

pub(crate) fn terminal_command(program: &str, arguments: &[String]) -> Result<Vec<String>, String> {
    if cargo_policy::setup(arguments)?.is_none() {
        return Ok(browser_guard::terminal_command(program, arguments));
    }
    let Some(setup) = setup(arguments)? else {
        return Ok(std::iter::once(program.to_string())
            .chain(arguments.iter().cloned())
            .collect());
    };
    Ok([
        "/bin/sh".into(),
        "-c".into(),
        format!("{} && exec \"$@\"", setup.command),
        "mike-fleet".into(),
        program.into(),
    ]
    .into_iter()
    .chain(arguments.iter().cloned())
    .collect())
}
