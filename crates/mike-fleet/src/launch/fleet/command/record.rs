use clap::Args;
use serde_json::Value;

use super::{Api, FleetError};

#[derive(Debug, Clone, Args)]
pub struct RecordArgs {
    #[arg(help = "The fleet's uuid or its slug")]
    pub fleet: String,
    #[arg(
        long,
        value_name = "FLEET",
        help = "Another fleet's uuid or slug: print the difference from it"
    )]
    pub against: Option<String>,
    #[arg(long, help = "Measure the run again and replace its stored record")]
    pub recompute: bool,
    #[arg(long, help = "Print the record as JSON, exactly as the API answered")]
    pub json: bool,
}

const ROWS: [(&str, &str); 20] = [
    ("time to done (min)", "/time_to_done_minutes"),
    ("minutes lost to faults", "/fault_minutes/total"),
    ("  no seat", "/fault_minutes/no_seat"),
    ("  relaunch gap", "/fault_minutes/relaunch_gap"),
    (
        "  report waiting for a verdict",
        "/fault_minutes/report_to_verdict_wait",
    ),
    ("  resource wait", "/fault_minutes/resource_wait"),
    ("  check past its timeout", "/fault_minutes/check_timeout"),
    ("  stopping", "/fault_minutes/stop"),
    (
        "  control seats' model time",
        "/fault_minutes/control_seat_model_time",
    ),
    (
        "tokens per accepted task",
        "/tokens_per_accepted_task/total",
    ),
    (
        "  tokens in the run",
        "/tokens_per_accepted_task/all_tokens",
    ),
    (
        "  accepted tasks",
        "/tokens_per_accepted_task/accepted_tasks",
    ),
    ("manual interventions", "/manual_interventions/total"),
    (
        "  operator turns in seats",
        "/manual_interventions/operator_turns",
    ),
    ("  hand-started seats", "/manual_interventions/hand_starts"),
    (
        "mail-driven turns in seats",
        "/manual_interventions/mail_driven_turns",
    ),
    (
        "  answered operator notes",
        "/manual_interventions/needs_you_answers",
    ),
    ("seats per task", "/counts/seats_per_task"),
    ("relaunches", "/counts/relaunches"),
    (
        "unread messages at close",
        "/counts/unread_messages_at_close",
    ),
];

fn cell(record: &Value, pointer: &str) -> String {
    match record.pointer(pointer) {
        Some(Value::Number(number)) => number.to_string(),
        _ => "-".to_string(),
    }
}

fn single(record: &Value) -> Vec<String> {
    ROWS.iter()
        .map(|(label, pointer)| format!("{label:<34}{:>16}", cell(record, pointer)))
        .collect()
}

fn compared(record: &Value, other: &Value, difference: &Value) -> Vec<String> {
    ROWS.iter()
        .map(|(label, pointer)| {
            format!(
                "{label:<34}{:>16}{:>16}{:>16}",
                cell(record, pointer),
                cell(other, pointer),
                cell(difference, pointer)
            )
        })
        .collect()
}

pub fn render_record(answer: &Value) -> String {
    let (record, against) = match answer.get("against") {
        Some(against) => (&answer["record"], Some(against)),
        None => (answer, None),
    };
    let version = record["measure_version"].as_str().unwrap_or("unknown");
    let mut lines = vec![format!("run record {version}")];
    match against {
        Some(other) => {
            lines.push(format!(
                "{:<34}{:>16}{:>16}{:>16}",
                "", "this run", "against", "difference"
            ));
            lines.extend(compared(record, &other["record"], &other["difference"]));
        }
        None => lines.extend(single(record)),
    }
    lines.join("\n")
}

pub async fn record_text(api: &Api, args: &RecordArgs) -> Result<String, FleetError> {
    let mut url = api.url(&[&args.fleet, "record"])?;
    {
        let mut query = url.query_pairs_mut();
        if args.recompute {
            query.append_pair("recompute", "true");
        }
        if let Some(other) = &args.against {
            query.append_pair("against", other);
        }
    }
    let body = api.get(url).await?;
    if args.json {
        return Ok(body.trim_end().to_string());
    }
    let answer: Value = api.parse(&body)?;
    Ok(render_record(&answer))
}
