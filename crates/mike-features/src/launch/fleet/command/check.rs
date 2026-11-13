use std::time::Duration;

use clap::Args;
use serde_json::json;
use uuid::Uuid;

use super::api::{Api, FleetError};
use super::commit::{own_fleet, own_session, resolve};
use crate::launch::fleet::check::{CheckReport, DEFAULT_WAIT_SECONDS, MAX_WAIT_SECONDS};
use crate::launch::fleet::check_report;
use crate::launch::fleet::locate::Located;

const ANSWER_MARGIN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Args)]
pub struct CheckArgs {
    #[arg(
        long,
        help = "The fleet's uuid, slug or run id; default: the MIKE_RUN_ID of this pane"
    )]
    pub fleet: Option<String>,
    #[arg(
        long,
        help = "The seat's session id; default: the MIKE_SESSION_ID of this pane"
    )]
    pub session: Option<Uuid>,
    #[arg(
        long,
        help = "Only read check status for the shared tree or the explicit --commit baseline; starts nothing"
    )]
    pub status: bool,
    #[arg(
        long,
        value_name = "SECONDS",
        default_value_t = DEFAULT_WAIT_SECONDS,
        help = "Optional wait, capped at 90 seconds; default returns a queued receipt immediately and mike mails the result"
    )]
    pub wait: u64,
    #[arg(
        long,
        value_name = "NAME",
        help = "Run only this one test, with the project's declared focused-test command (definition `check.test_command`), instead of the whole check"
    )]
    pub test: Option<String>,
    #[arg(long, value_name = "COMMAND", conflicts_with_all = ["test", "ruling"], help = "Queue this exact project operation; use its declared resources and environment") ]
    pub command: Option<String>,
    #[arg(
        long,
        value_name = "COMMIT",
        help = "Run a clean baseline at this commit (including HEAD) in mike's reusable checkout; omitted: check the shared tree"
    )]
    pub commit: Option<String>,
    #[arg(
        long,
        value_name = "NOTE_UUID",
        help = "Run this registered acceptance ruling at --commit; returns durable proof for fleet_report"
    )]
    pub ruling: Option<Uuid>,
}

pub fn check_body(session: Uuid, args: &CheckArgs) -> serde_json::Value {
    let mut body = json!({"session": session, "status": args.status, "wait_seconds": args.wait.min(MAX_WAIT_SECONDS)});
    if let Some(command) = &args.command {
        body["command"] = json!(command);
    }
    if let Some(test) = &args.test {
        body["test"] = json!(test);
    }
    if let Some(ruling) = args.ruling {
        body["ruling"] = json!(ruling);
    }
    if let Some(commit) = &args.commit {
        body["commit"] = json!(commit);
    }
    body
}

#[derive(Debug, Clone, Args)]
pub struct WhereArgs {
    #[arg(help = "The name to find the definition of")]
    pub symbol: Option<String>,
    #[arg(
        long,
        help = "A file or directory to look in, or to list the definitions of"
    )]
    pub path: Option<String>,
    #[arg(
        long,
        help = "The fleet's uuid, slug or run id; default: the MIKE_RUN_ID of this pane"
    )]
    pub fleet: Option<String>,
}

pub async fn check_text(api: &Api, args: &CheckArgs) -> Result<String, FleetError> {
    let session = own_session(args.session)?;
    let fleet = resolve(api, &own_fleet(args.fleet.as_deref())?).await?;
    let body = check_body(session, args);
    let patience = Duration::from_secs(args.wait.min(MAX_WAIT_SECONDS)) + ANSWER_MARGIN;
    let answer = api
        .post_json_and_wait(api.url(&[&fleet, "check"])?, &body, patience)
        .await?;
    let report: CheckReport = api.parse(&answer)?;
    Ok(check_report::text(&report))
}

pub async fn where_text(api: &Api, args: &WhereArgs) -> Result<String, FleetError> {
    let fleet = resolve(api, &own_fleet(args.fleet.as_deref())?).await?;
    let mut url = api.url(&[&fleet, "where"])?;
    if let Some(symbol) = &args.symbol {
        url.query_pairs_mut().append_pair("symbol", symbol);
    }
    if let Some(path) = &args.path {
        url.query_pairs_mut().append_pair("path", path);
    }
    let answer = api.get(url).await?;
    let located: Located = api.parse(&answer)?;
    Ok(render_where(&located))
}

pub fn render_where(located: &Located) -> String {
    let mut lines: Vec<String> = located
        .definitions
        .iter()
        .map(|d| format!("{}:{} {}", d.path, d.line, d.text))
        .collect();
    lines.extend(located.files_named.iter().map(|f| format!("{f} (file)")));
    if located.definitions_truncated {
        lines.push("(more definitions than shown; narrow with --path)".to_string());
    }
    if lines.is_empty() {
        return "not found".to_string();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::fleet::check::Blocking;
    use crate::launch::fleet::diagnostics::Diagnostic;

    fn args(test: Option<&str>, commit: Option<&str>) -> CheckArgs {
        CheckArgs {
            fleet: None,
            session: None,
            status: false,
            wait: 45,
            test: test.map(str::to_string),
            command: None,
            commit: commit.map(str::to_string),
            ruling: None,
        }
    }

    #[test]
    fn a_named_test_and_commit_are_sent_only_when_given() {
        let session = Uuid::nil();
        let plain = check_body(session, &args(None, None));
        assert!(plain.get("test").is_none() && plain.get("commit").is_none());
        let focused = check_body(session, &args(Some("mike::cli x"), Some("abc1234")));
        assert_eq!(focused["test"], json!("mike::cli x"));
        assert_eq!(focused["commit"], json!("abc1234"));
        assert_eq!(focused["wait_seconds"], json!(45));
    }

    #[test]
    fn explicit_commands_and_zero_wait_are_preserved_and_long_waits_are_capped() {
        let mut ask = args(None, None);
        ask.command = Some("builder nextest run -p gg --test mix".into());
        ask.wait = 0;
        let body = check_body(Uuid::nil(), &ask);
        assert_eq!(
            body["command"],
            json!("builder nextest run -p gg --test mix")
        );
        assert_eq!(body["wait_seconds"], json!(0));
        ask.wait = 1700;
        assert_eq!(check_body(Uuid::nil(), &ask)["wait_seconds"], json!(90));
    }

    #[test]
    fn a_check_report_names_whose_files_fail() {
        let report = CheckReport {
            receipt_id: None,
            command_receipt: false,
            baseline: false,
            seat: "db-dev-T1".into(),
            command: "make check".into(),
            state: "0123456789abcdef".into(),
            commit: "fedcba98765432100000".into(),
            how: "reused".into(),
            passed: false,
            exit_code: Some(1),
            seconds: 42,
            askers: vec!["db-dev-T1".into(), "db-dev-T2".into()],
            mine: vec![Diagnostic {
                path: "a.py".into(),
                line: 3,
                text: "a.py:3: error".into(),
            }],
            others: vec![Blocking {
                path: "b.py".into(),
                line: 9,
                text: "b.py:9: error".into(),
                seat: "db-dev-T2".into(),
                task: "T2".into(),
            }],
            unowned: vec![],
            owned: vec![],
            output_tail: String::new(),
            refused: None,
            started_at: None,
        };
        assert_eq!(
            check_report::text(&report),
            "`make check` failed at commit fedcba9876 (state 0123456789ab; reused, 42 s, asked by db-dev-T1, db-dev-T2)\nin your files:\n  a.py:3 a.py:3: error\nin other seats' files (that seat has been told):\n  b.py:9 [db-dev-T2 / T2] b.py:9: error"
        );
    }

    #[test]
    fn a_check_still_running_says_so_and_that_the_result_comes_by_mail() {
        let running = CheckReport {
            receipt_id: None,
            command_receipt: false,
            baseline: false,
            seat: "db-dev-T1".into(),
            command: "make check".into(),
            state: "0123456789abcdef".into(),
            commit: "fedcba98765432100000".into(),
            how: "running".into(),
            passed: false,
            exit_code: None,
            seconds: 95,
            askers: vec![],
            mine: vec![],
            others: vec![],
            unowned: vec![],
            owned: vec![],
            output_tail: String::new(),
            refused: None,
            started_at: "2026-10-02T13:00:05Z".parse().ok(),
        };
        assert_eq!(
            check_report::text(&running),
            "`make check` is running at commit fedcba9876 (state 0123456789ab), for 95 s so far (since 13:00:05Z). If you asked for the check, mike mails you its result when it ends: carry on with work that needs no build and do not wait in a loop"
        );
        let not_run = CheckReport {
            how: "not run".into(),
            seconds: 0,
            started_at: None,
            ..running
        };
        assert_eq!(
            check_report::text(&not_run),
            "`make check` has not run at commit fedcba9876 (state 0123456789ab) and is not running now; mike starts it by itself within a minute of a commit, or ask again without the status form to start it now"
        );
    }

    #[test]
    fn a_where_answer_lists_definitions_and_files() {
        let located = Located {
            symbol: Some("Foo".into()),
            path: None,
            definitions: vec![crate::launch::fleet::locate::Definition {
                path: "src/foo.rs".into(),
                line: 4,
                text: "pub struct Foo {".into(),
            }],
            files_named: vec!["ui/Foo.svelte".into()],
            definitions_truncated: false,
        };
        assert_eq!(
            render_where(&located),
            "src/foo.rs:4 pub struct Foo {\nui/Foo.svelte (file)"
        );
        assert_eq!(render_where(&Located::default()), "not found");
    }
}
