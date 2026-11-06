use super::channel;
use super::doctor::DoctorReport;
use super::kill::KillReport;
use super::list::ListRow;
use super::open::{self, OpenFailure, OpenResult};
use super::reap::ReapReport;
use super::templates;
use super::warm::WarmOutcome;
use crate::launch::LaunchError;
use crate::store::persistence::launch::TemplateRow;

pub(crate) const CHOOSE_BACKEND: &str = "choose a terminal backend: run `mike` and complete the \
                                         terminal step of setup, or set MIKE_LAUNCH_BACKEND";

pub(crate) fn report_error(error: &LaunchError) {
    eprintln!("mike spawn: {error}");
    let remedy = match error {
        LaunchError::BackendNone => Some(CHOOSE_BACKEND),
        other => other.remedy(),
    };
    if let Some(remedy) = remedy {
        eprintln!("mike spawn: {remedy}");
    }
}

pub(crate) fn print_open(result: &OpenResult, json: bool) -> i32 {
    match result {
        Ok(output) => {
            if json {
                println!("{}", output.to_json_line());
            } else {
                if let Some(workspace) = &output.workspace {
                    println!("workspace: {workspace}");
                }
                if let Some(surface) = &output.surface {
                    println!("surface: {surface}");
                }
                if let Some(session) = &output.session {
                    println!("session: {session}");
                }
                println!("channel: {}", channel::as_str(output.channel_check));
            }
            if let Some(reason) = &output.reason {
                eprintln!("mike spawn: launch failed: {reason} (the surface is left open)");
            }
        }
        Err(failure) => {
            if let (true, Some(output)) = (json, &failure.output) {
                println!("{}", output.to_json_line());
            }
            report_error(&failure.error);
        }
    }
    open::exit_code(result)
}

pub(crate) fn print_kill(report: &KillReport, json: bool) -> i32 {
    if json {
        println!("{}", report.to_json_line());
    } else {
        match &report.surface {
            Some(surface) if report.closed => println!(
                "closed: {surface} process_exited: {}",
                report
                    .process_exited
                    .map_or("unknown".to_string(), |b| b.to_string())
            ),
            _ => println!("nothing to do"),
        }
    }
    if let Some(note) = &report.note {
        eprintln!("mike spawn: {note}");
    }
    0
}

pub(crate) fn print_list(rows: &[ListRow], json: bool) -> i32 {
    if json {
        println!(
            "{}",
            serde_json::to_string(rows)
                .unwrap_or_else(|e| format!("[\"json encoding failed: {e}\"]"))
        );
        return 0;
    }
    eprintln!("surface\tfolder\tsession\tagent_state\trun/seat");
    for row in rows {
        println!("{}", row.to_line());
    }
    0
}

pub(crate) fn print_warm(outcome: &Result<WarmOutcome, OpenFailure>, json: bool) -> i32 {
    match outcome {
        Ok(warm) => {
            if let Some(replaced) = warm.replaced_session {
                eprintln!("mike spawn warm: replaced template session {replaced}");
            }
            print_open(&warm.result, json)
        }
        Err(failure) => {
            if let (true, Some(output)) = (json, &failure.output) {
                println!("{}", output.to_json_line());
            }
            report_error(&failure.error);
            failure.error.exit_code()
        }
    }
}

pub(crate) fn print_templates_list(rows: &[TemplateRow], json: bool) -> i32 {
    if json {
        println!(
            "{}",
            serde_json::to_string(rows)
                .unwrap_or_else(|e| format!("[\"json encoding failed: {e}\"]"))
        );
        return 0;
    }
    eprintln!("name\tsession\tfolder\tmodel\tage\ttranscript");
    for row in rows {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            row.name,
            row.session_uuid,
            row.folder,
            row.model.as_deref().unwrap_or("-"),
            templates::age(row),
            templates::transcript_exists(row),
        );
    }
    0
}

pub(crate) fn print_templates_show(row: &TemplateRow, json: bool) -> i32 {
    if json {
        println!(
            "{}",
            serde_json::to_string(row)
                .unwrap_or_else(|e| format!("{{\"error\":\"json encoding failed: {e}\"}}"))
        );
        return 0;
    }
    println!("name: {}", row.name);
    println!("session: {}", row.session_uuid);
    println!("folder: {}", row.folder);
    println!("backend: {}", row.backend);
    if let Some(model) = &row.model {
        println!("model: {model}");
    }
    if let Some(agent) = &row.agent {
        println!("agent: {agent}");
    }
    println!("created: {}", row.created_at);
    println!("transcript: {}", templates::transcript_exists(row));
    println!("claude_args: {}", row.claude_args);
    0
}

pub(crate) fn print_templates_delete(name: &str, closed: bool) -> i32 {
    if closed {
        println!("closed: {name}");
    } else {
        println!("nothing to do: no live template named {name}");
    }
    0
}

pub(crate) fn print_reap(report: &ReapReport, json: bool) -> i32 {
    if json {
        println!("{}", report.to_json_line());
        return i32::from(report.killed.iter().any(|k| !k.killed));
    }
    for row in &report.to_kill {
        println!(
            "{}\tpid {}\t{}\t{}",
            row.host,
            row.pid,
            row.session.as_deref().unwrap_or("-"),
            row.cwd,
        );
    }
    for row in &report.skipped_self {
        eprintln!(
            "mike spawn reap: skipping pid {} — it is the caller or an ancestor ({})",
            row.pid, row.host
        );
    }
    if report.killed.is_empty() {
        if !report.to_kill.is_empty() {
            eprintln!("mike spawn reap: dry run — pass --yes to kill these");
        }
        return 0;
    }
    let mut failed = 0;
    for row in &report.killed {
        if row.killed {
            println!("killed: {}", row.pid);
        } else {
            failed += 1;
            eprintln!(
                "mike spawn reap: could not kill {}: {}",
                row.pid,
                row.detail.as_deref().unwrap_or("unknown")
            );
        }
    }
    i32::from(failed > 0)
}

pub(crate) fn print_doctor(report: &DoctorReport, json: bool) -> i32 {
    if json {
        println!("{}", report.to_json_line());
    } else {
        println!("backend: {}", report.backend);
        for check in &report.checks {
            println!(
                "{} {} — {}",
                if check.passed { "PASS" } else { "FAIL" },
                check.name,
                check.detail
            );
            if let (false, Some(remedy)) = (check.passed, &check.remedy) {
                println!("     remedy: {remedy}");
            }
        }
        println!(
            "doctor: {}",
            if report.passed {
                "clean"
            } else {
                "problems found"
            }
        );
    }
    i32::from(!report.passed)
}
