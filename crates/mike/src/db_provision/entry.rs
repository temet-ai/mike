use super::DbError;
use super::native::{self, Platform};
use super::runner::SystemRunner;
use super::up::Timing;
use crate::doctor::{Report, Status, database_report};

pub(crate) async fn start_brew_database() -> Result<String, String> {
    let platform = native::detect(std::env::consts::OS, &crate::tui::on_path);
    if platform != Platform::MacWithBrew {
        return Err(
            "mike can run these steps only on macOS with Homebrew. Run `mike db native` in a terminal to see the commands for this machine"
                .to_string(),
        );
    }
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .map_err(|_| "neither USER nor LOGNAME is set".to_string())?;
    let plan = native::plan(&platform, &user);
    native::execute(&SystemRunner, &plan, Timing::REAL)
        .await
        .map_err(|e: DbError| e.to_string())?;
    verify_existing_database(&plan.url).await?;
    Ok(plan.url)
}

pub(crate) async fn verify_existing_database(url: &str) -> Result<(), String> {
    first_failure(&database_report(url).await).map_or(Ok(()), Err)
}

pub(super) fn first_failure(report: &Report) -> Option<String> {
    report
        .checks
        .iter()
        .find(|check| check.status == Status::Fail)
        .map(|check| {
            format!(
                "{}: {} (run `mike db check` for every check)",
                check.name, check.detail
            )
        })
}
