use std::io::{BufRead, Write};
use std::path::Path;

use anyhow::Context;

use crate::db_provision::{DownOutcome, Paths, Runner, Timing, VOLUME, down, remove_volume};

fn confirm(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    question: &str,
) -> anyhow::Result<bool> {
    write!(output, "{question} [y/N] ")?;
    output.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub(super) async fn purge<R: Runner>(
    runner: &R,
    config_file: &Path,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> anyhow::Result<i32> {
    let paths = Paths {
        config_file: config_file.to_path_buf(),
        state_dir: crate::user_config::state_dir().context("cannot find the state folder")?,
    };
    purge_with(runner, &paths, Timing::REAL, input, output).await
}

pub(super) async fn purge_with<R: Runner>(
    runner: &R,
    paths: &Paths,
    timing: Timing,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> anyhow::Result<i32> {
    let mut code = 0;
    match down(runner, paths, timing).await {
        Ok(DownOutcome::Stopped) => writeln!(output, "Stopped the database container.")?,
        Ok(DownOutcome::NothingToStop) => {
            writeln!(
                output,
                "`mike db up` never set up a database here, so there was no container to stop."
            )?;
        }
        Err(e) => {
            writeln!(output, "Could not stop the database container: {e}")?;
            code = 1;
        }
    }
    let volume_question =
        format!("Delete the Docker volume {VOLUME}, which holds every capture mike stored in it?");
    if confirm(input, output, &volume_question)? {
        match remove_volume(runner).await {
            Ok(()) => writeln!(output, "Deleted the Docker volume {VOLUME}.")?,
            Err(e) => {
                writeln!(output, "Could not delete the volume: {e}")?;
                code = 1;
            }
        }
    } else {
        writeln!(output, "Kept the Docker volume {VOLUME}.")?;
    }
    let file_question = format!(
        "Delete the config file {}, which holds the database URL and any key saved there?",
        paths.config_file.display()
    );
    if confirm(input, output, &file_question)? {
        match std::fs::remove_file(&paths.config_file) {
            Ok(()) => writeln!(output, "Deleted {}.", paths.config_file.display())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                writeln!(output, "{} does not exist.", paths.config_file.display())?;
            }
            Err(e) => {
                writeln!(
                    output,
                    "Could not delete {}: {e}",
                    paths.config_file.display()
                )?;
                code = 1;
            }
        }
    } else {
        writeln!(output, "Kept {}.", paths.config_file.display())?;
    }
    writeln!(
        output,
        "The state folder {} was left alone; it holds logs and launch records.",
        paths.state_dir.display()
    )?;
    Ok(code)
}
