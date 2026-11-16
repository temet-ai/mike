pub(crate) const WRIGHT_ENGINEER: &str = "wright-engineer";

const BY_PREFIX: [(&str, &str); 6] = [
    ("frontend/", "frontend-developer"),
    ("scripts/release/", "release-tooling-developer"),
    ("scripts/", "scripts-developer"),
    ("docs/user-guide/", "user-guide-writer"),
    ("docs/", "docs-writer"),
    (".sqlx/", "db-dev"),
];

pub(crate) fn for_part(part: &str) -> &'static str {
    let part = part.trim().trim_start_matches("./");
    if part.contains("/migrations/") || part.ends_with(".sql") {
        return "db-dev";
    }
    BY_PREFIX
        .iter()
        .find(|(prefix, _)| part.starts_with(prefix))
        .map_or("rust-developer", |(_, agent)| agent)
}

#[cfg(test)]
mod tests {
    use super::for_part;

    #[test]
    fn specialist_for_each_owning_part_is_the_repo_agent_for_that_area() {
        let cases = [
            ("crates/mike-mcp/src/tools/arg_check.rs", "rust-developer"),
            (
                "crates/mike/src/store/persistence/wright/sql/repairs_claim.sql",
                "db-dev",
            ),
            (
                "crates/mike-base/src/store/persistence/migrations/x.sql",
                "db-dev",
            ),
            (
                "frontend/src/routes/wright/+page.svelte",
                "frontend-developer",
            ),
            ("scripts/release/acceptance.py", "release-tooling-developer"),
            ("scripts/pre-push-check", "scripts-developer"),
            ("docs/user-guide/pages.md", "user-guide-writer"),
            ("docs/ARCHITECTURE.md", "docs-writer"),
            ("", "rust-developer"),
        ];
        for (part, agent) in cases {
            assert_eq!(for_part(part), agent, "{part}");
        }
    }
}
