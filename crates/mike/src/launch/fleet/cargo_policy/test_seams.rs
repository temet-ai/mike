pub use super::cargo_resolution::cases as resolution;
pub use super::classifier::cases as classifier;
pub use super::install::cases as settings;
pub use super::subprocess_cases as subprocess;

pub fn combined_terminal_command(program: &str, args: &[String]) -> Result<Vec<String>, String> {
    crate::launch::fleet::seat_helpers::terminal_command(program, args)
}

pub fn prepared_settings(
    definition: &crate::launch::fleet::FleetDefinition,
    scratch: &std::path::Path,
    seat: &str,
    helper: &std::path::Path,
) -> Result<serde_json::Value, String> {
    std::fs::create_dir_all(scratch).map_err(|error| error.to_string())?;
    let settings = crate::launch::fleet::seat_settings::effective(definition, scratch, seat);
    crate::launch::fleet::browser_guard::prepare(&settings)?;
    super::install::prepare(&settings, helper)?;
    Ok(settings)
}
