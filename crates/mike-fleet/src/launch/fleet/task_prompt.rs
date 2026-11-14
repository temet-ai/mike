use std::path::{Path, PathBuf};

use super::definition::FleetDefinition;
use super::plan::SeatPlan;

const FLAG: &str = "--append-system-prompt-file";
const PROMPTS: &str = "prompts";

#[cfg(test)]
#[path = "task_prompt_tests.rs"]
mod tests;

pub fn text(def: &FleetDefinition, seat: &SeatPlan, contract: Option<&str>) -> Option<String> {
    let id = seat.task.as_deref()?;
    let task = def.tasks.get(id)?;
    let prompt = task.system_prompt.trim();
    if prompt.is_empty() {
        return None;
    }
    let mut out = String::new();
    if let Some(contract) = contract.map(str::trim).filter(|c| !c.is_empty()) {
        out.push_str(contract);
        out.push_str("\n\n");
    }
    out.push_str(&format!("# Task {id}, seat {}\n\n{prompt}\n", seat.seat));
    let files: Vec<&str> = task
        .files
        .iter()
        .chain(&task.new_files)
        .map(|f| f.trim())
        .filter(|f| !f.is_empty())
        .collect();
    if !files.is_empty() {
        out.push_str(&format!("\nFiles this task owns: {}\n", files.join(", ")));
    }
    Some(out)
}

pub fn appended_file(args: &[String]) -> Option<&str> {
    let at = args.iter().position(|arg| arg == FLAG)?;
    args.get(at + 1).map(String::as_str)
}

pub fn with_file(args: Vec<String>, file: &Path) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len() + 2);
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == FLAG {
            iter.next();
            continue;
        }
        out.push(arg);
    }
    out.push(FLAG.to_string());
    out.push(file.to_string_lossy().into_owned());
    out
}

pub fn prompts_dir(scratch: &Path) -> PathBuf {
    scratch.join(PROMPTS)
}

pub fn apply(
    def: &FleetDefinition,
    seat: &SeatPlan,
    args: Vec<String>,
    dir: &Path,
) -> Result<Vec<String>, String> {
    let contract = appended_file(&args).and_then(|file| std::fs::read_to_string(file).ok());
    let Some(text) = text(def, seat, contract.as_deref()) else {
        return Ok(args);
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let file = dir.join(format!("{}.md", seat.session));
    std::fs::write(&file, text).map_err(|e| format!("could not write {}: {e}", file.display()))?;
    Ok(with_file(args, &file))
}
