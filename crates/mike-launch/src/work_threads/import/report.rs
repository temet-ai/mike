use std::fmt::Write;

#[derive(Debug, Default)]
pub struct Report {
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub left_alone: Vec<(String, String)>,
    pub parse_errors: Vec<(String, String)>,
    pub skipped_repos: Vec<(String, String)>,
    pub unresolved_dependencies: Vec<(String, Vec<String>)>,
    pub registry_problems: Vec<String>,
    pub log_entries: usize,
}

fn block(out: &mut String, heading: &str, lines: impl Iterator<Item = String>) {
    let lines: Vec<String> = lines.collect();
    if lines.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n{heading} ({}):", lines.len());
    for line in lines {
        let _ = writeln!(out, "  {line}");
    }
}

impl Report {
    pub fn render(&self, dry_run: bool) -> String {
        let verb = if dry_run { "would be " } else { "" };
        let mut out = String::new();
        if dry_run {
            let _ = writeln!(out, "Dry run: nothing was written to the database.");
        }
        let _ = writeln!(
            out,
            "Threads {verb}created: {}\nThreads {verb}updated: {}\nSession log entries read: {}\nThreads with a parse error: {}\nRepositories skipped: {}",
            self.created.len(),
            self.updated.len(),
            self.log_entries,
            self.parse_errors.len(),
            self.skipped_repos.len(),
        );
        block(
            &mut out,
            "Parse errors (the thread still has a row)",
            self.parse_errors
                .iter()
                .map(|(path, error)| format!("{path}: {error}")),
        );
        block(
            &mut out,
            "Skipped repositories",
            self.skipped_repos
                .iter()
                .map(|(repo, why)| format!("{repo}: {why}")),
        );
        block(
            &mut out,
            "Left alone",
            self.left_alone
                .iter()
                .map(|(thread, why)| format!("{thread}: {why}")),
        );
        block(
            &mut out,
            "Dependencies naming no known thread",
            self.unresolved_dependencies
                .iter()
                .map(|(thread, names)| format!("{thread}: {}", names.join(", "))),
        );
        block(
            &mut out,
            "Registry lines that could not be used",
            self.registry_problems.iter().cloned(),
        );
        out
    }
}
