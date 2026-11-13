use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::plan::{REFERENCE_SEAT, SeatPlan, template_name};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetSeatRow;
use crate::store::persistence::launch::TemplateRow;

pub const DEFAULT_TEMPLATE_LIMIT_TOKENS: i64 = 500_000;
const CACHE_LIFETIME_SECONDS: i64 = 3600;

pub async fn template_for(
    persist: &PersistHandle,
    run_id: &str,
    seat: &SeatPlan,
    args: &[String],
) -> Option<TemplateRow> {
    if seat.task.is_none() && seat.seat != REFERENCE_SEAT {
        return None;
    }
    let discipline = seat.discipline.as_deref()?;
    let found = persist
        .launch()
        .find_live_template_by_name(&template_name(run_id, discipline))
        .await;
    let own = usable(found.map_err(|e| e.to_string()), seat, args);
    let row = match own {
        Some(row) => row,
        None => nearest(persist, run_id, seat, args).await?,
    };
    let seat_row = template_seat(persist, run_id, &row).await;
    match cache_expired(seat_row.as_ref(), Utc::now()) {
        Some(why) => {
            cache_cold(&row, seat, &why);
            None
        }
        None => Some(row),
    }
}

async fn nearest(
    persist: &PersistHandle,
    run_id: &str,
    seat: &SeatPlan,
    args: &[String],
) -> Option<TemplateRow> {
    match persist.launch().list_live_templates().await {
        Ok(rows) => nearest_matching(rows, run_id, args),
        Err(err) => {
            lookup_failed(&err.to_string(), seat);
            None
        }
    }
}

pub fn nearest_matching(
    rows: Vec<TemplateRow>,
    run_id: &str,
    args: &[String],
) -> Option<TemplateRow> {
    let prefix = template_name(run_id, "");
    rows.into_iter()
        .filter(|row| row.name.starts_with(&prefix) && matches(row, args))
        .max_by_key(|row| row.created_at)
}

async fn template_seat(
    persist: &PersistHandle,
    run_id: &str,
    template: &TemplateRow,
) -> Option<FleetSeatRow> {
    persist
        .fleet()
        .fleet_seat_rows(run_id)
        .await
        .ok()?
        .into_iter()
        .find(|r| r.session_uuid == Some(template.session_uuid))
}

pub fn cache_expired(template: Option<&FleetSeatRow>, now: DateTime<Utc>) -> Option<String> {
    let last = template?.last_request_at?;
    let age = now - last;
    (age > Duration::seconds(CACHE_LIFETIME_SECONDS)).then(|| {
        format!(
            "the template's last request was {} minutes ago, past the {} minutes a cached prefix lives",
            age.num_minutes(),
            CACHE_LIFETIME_SECONDS / 60
        )
    })
}

fn cache_cold(row: &TemplateRow, seat: &SeatPlan, why: &str) {
    tracing::warn!(template = %row.name, seat = %seat.seat, %why, "this run's template cache has expired; the seat starts cold instead of writing the whole template again");
}

fn usable(
    found: Result<Option<TemplateRow>, String>,
    seat: &SeatPlan,
    args: &[String],
) -> Option<TemplateRow> {
    let row = found.map_err(|err| lookup_failed(&err, seat)).ok()??;
    if matches(&row, args) {
        return Some(row);
    }
    warmed_otherwise(&row, seat);
    None
}

fn lookup_failed(err: &str, seat: &SeatPlan) {
    tracing::warn!(error = %err, seat = %seat.seat, "mike could not look up this run's warm template; the seat starts cold");
}

fn warmed_otherwise(row: &TemplateRow, seat: &SeatPlan) {
    tracing::info!(template = %row.name, seat = %seat.seat, "this run's template was warmed with other arguments; the seat starts cold");
}

pub fn over_limit(limit: Option<i64>, template_tokens: Option<i64>) -> Option<String> {
    let limit = limit.unwrap_or(DEFAULT_TEMPLATE_LIMIT_TOKENS);
    let tokens = template_tokens?;
    (tokens > limit).then(|| {
        format!(
            "the template's context is {tokens} tokens, over the limit of {limit} tokens (the definition's template_limit_tokens, {DEFAULT_TEMPLATE_LIMIT_TOKENS} when it sets none); mike does not fork from it"
        )
    })
}

pub async fn refuse_over_limit(
    persist: &PersistHandle,
    def: &super::definition::FleetDefinition,
    run_id: &str,
    template: &TemplateRow,
) -> Option<String> {
    let tokens = template_seat(persist, run_id, template)
        .await
        .and_then(|r| r.context_tokens);
    over_limit(def.template_limit_tokens, tokens)
}

pub fn matches(row: &TemplateRow, args: &[String]) -> bool {
    serde_json::from_value::<Vec<String>>(row.claude_args.clone())
        .is_ok_and(|recorded| without_tool_profile(&recorded) == without_tool_profile(args))
}

fn without_tool_profile(args: &[String]) -> Vec<String> {
    let mut out = args.to_vec();
    let settings = out
        .iter()
        .position(|arg| arg == "--settings")
        .map(|at| at + 1);
    if let Some(text) = settings.and_then(|at| out.get_mut(at))
        && let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(env) = value.get_mut("env").and_then(|env| env.as_object_mut())
    {
        env.remove(super::seat_settings::PROFILE_ENV);
        *text = value.to_string();
    }
    out
}

pub fn fork_args(row: &TemplateRow, session: Uuid, args: Vec<String>) -> Vec<String> {
    let mut out = vec![
        "--resume".to_string(),
        row.session_uuid.to_string(),
        "--fork-session".to_string(),
    ];
    out.extend(own_session(session));
    out.extend(args);
    out
}

pub fn cold_args(session: Uuid, args: Vec<String>) -> Vec<String> {
    let mut out = own_session(session);
    out.extend(args);
    out
}

fn own_session(session: Uuid) -> Vec<String> {
    vec!["--session-id".to_string(), session.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(args: &[&str]) -> TemplateRow {
        TemplateRow {
            id: 1,
            name: "fleet-run-1:rust-developer".into(),
            session_uuid: Uuid::nil(),
            folder: "/repo".into(),
            backend: "herdr".into(),
            model: Some("sonnet".into()),
            agent: Some("rust-developer".into()),
            claude_args: serde_json::json!(args),
            created_at: chrono::Utc::now(),
            superseded_by: None,
            closed_at: None,
        }
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_string()).collect()
    }

    #[test]
    fn a_template_over_the_limit_is_refused_with_both_numbers() {
        let refusal = over_limit(Some(100_000), Some(150_000)).unwrap_or_default();
        assert!(refusal.contains("150000") && refusal.contains("100000"));
    }

    #[test]
    fn a_template_within_the_limit_or_of_unknown_size_is_not_refused() {
        assert_eq!(over_limit(Some(100_000), Some(100_000)), None);
        assert_eq!(over_limit(Some(100_000), None), None);
        assert_eq!(over_limit(None, Some(DEFAULT_TEMPLATE_LIMIT_TOKENS)), None);
    }

    #[test]
    fn a_definition_without_a_limit_still_refuses_a_template_past_the_default() {
        let refusal = over_limit(None, Some(608_626)).unwrap_or_default();
        assert!(
            refusal.contains("608626") && refusal.contains("500000"),
            "{refusal}"
        );
    }

    fn named(name: &str, args: &[&str], created: &str) -> TemplateRow {
        TemplateRow {
            name: name.into(),
            created_at: created.parse().unwrap_or_default(),
            ..row(args)
        }
    }

    #[test]
    fn a_discipline_without_a_template_forks_the_newest_one_of_the_run_with_its_arguments() {
        let seat = ["--model", "sonnet", "--agent", "fleet-worker"];
        let rows = vec![
            named("fleet-run-1:sim", &seat, "2026-10-04T10:00:00Z"),
            named("fleet-run-1:client", &seat, "2026-10-04T10:05:00Z"),
            named(
                "fleet-run-1:assets",
                &["--model", "opus"],
                "2026-10-04T10:06:00Z",
            ),
            named("fleet-run-2:sim", &seat, "2026-10-04T10:07:00Z"),
        ];
        let found = nearest_matching(rows, "fleet-run-1", &strings(&seat));
        assert_eq!(
            found.map(|r| r.name),
            Some("fleet-run-1:client".to_string())
        );
    }

    #[test]
    fn no_template_of_the_run_with_the_same_arguments_leaves_the_seat_cold() {
        let rows = vec![named(
            "fleet-run-1:sim",
            &["--model", "opus"],
            "2026-10-04T10:00:00Z",
        )];
        assert!(nearest_matching(rows, "fleet-run-1", &strings(&["--model", "sonnet"])).is_none());
    }

    fn template_seat(last: Option<&str>) -> FleetSeatRow {
        FleetSeatRow {
            seat: Some("template-sim".into()),
            role: None,
            agent: None,
            model: None,
            session_uuid: Some(Uuid::nil()),
            surface_ref: None,
            opened_at: None,
            closed_at: None,
            requests: 11,
            last_request_at: last.map(|t| t.parse().unwrap_or_default()),
            context_tokens: Some(155_278),
            last_status: None,
            forked: false,
            first_cache_read: None,
            first_cache_written: None,
        }
    }

    #[test]
    fn a_template_whose_last_request_is_older_than_the_cache_lifetime_is_expired() {
        let now: DateTime<Utc> = "2026-10-04T12:00:00Z".parse().unwrap_or_default();
        let old = template_seat(Some("2026-10-04T10:30:00Z"));
        let why = cache_expired(Some(&old), now).unwrap_or_default();
        assert!(
            why.contains("90 minutes") && why.contains("60 minutes"),
            "{why}"
        );
        let fresh = template_seat(Some("2026-10-04T11:30:00Z"));
        assert_eq!(cache_expired(Some(&fresh), now), None);
    }

    #[test]
    fn a_template_with_no_recorded_request_is_not_called_expired() {
        let now: DateTime<Utc> = "2026-10-04T12:00:00Z".parse().unwrap_or_default();
        assert_eq!(cache_expired(Some(&template_seat(None)), now), None);
        assert_eq!(cache_expired(None, now), None);
    }

    #[test]
    fn a_template_serves_a_seat_only_with_the_same_arguments() {
        let seat = ["--model", "sonnet", "--agent", "rust-developer"];
        assert!(matches(&row(&seat), &strings(&seat)));
        assert!(!matches(
            &row(&["--model", "opus", "--agent", "rust-developer"]),
            &strings(&seat)
        ));
    }

    #[test]
    fn a_fork_resumes_the_template_under_the_session_the_fleet_chose() {
        let session = Uuid::from_u128(7);
        let args = fork_args(&row(&[]), session, strings(&["--model", "sonnet"]));
        assert_eq!(
            args,
            strings(&[
                "--resume",
                "00000000-0000-0000-0000-000000000000",
                "--fork-session",
                "--session-id",
                "00000000-0000-0000-0000-000000000007",
                "--model",
                "sonnet"
            ])
        );
    }

    #[test]
    fn a_cold_seat_still_starts_under_the_session_the_fleet_chose() {
        let session = Uuid::from_u128(9);
        assert_eq!(
            cold_args(session, strings(&["--model", "opus"])),
            strings(&[
                "--session-id",
                "00000000-0000-0000-0000-000000000009",
                "--model",
                "opus"
            ])
        );
    }
}
