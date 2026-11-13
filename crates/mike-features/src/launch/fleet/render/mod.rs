pub mod attention;
pub mod clock;
pub mod head;
pub mod ink;
pub mod listing;
pub mod seats;
pub mod tasks;
#[cfg(test)]
pub mod tests;
#[cfg(test)]
pub mod tests_fixture;
pub mod verdicts;

pub use listing::{FleetListing, render_list};

pub use ink::strip;

use super::status::FleetStatus;

pub const DEFAULT_WIDTH: usize = 170;

pub fn render(status: &FleetStatus, width: usize) -> String {
    let sections = [
        head::missing(status),
        head::header(status, width),
        attention::attention(status),
        tasks::tasks(status),
        seats::seats(status),
        verdicts::verdicts(status),
        verdicts::blocked(status),
    ];
    let mut lines: Vec<String> = Vec::new();
    for section in sections.into_iter().filter(|s| !s.is_empty()) {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend(section);
    }
    lines
        .iter()
        .map(|line| ink::fit(line, width))
        .collect::<Vec<_>>()
        .join("\n")
}
