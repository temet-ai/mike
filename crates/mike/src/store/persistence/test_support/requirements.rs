pub(super) fn admin_url() -> anyhow::Result<String> {
    std::env::var("MIKE_TEST_ADMIN_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!(
            "this selected test requires MIKE_TEST_ADMIN_URL for an explicit disposable PostgreSQL/pgvector instance; run scripts/test-db/up.sh and set the test URL. Required database assertions were NOT RUN"
        ))
}
