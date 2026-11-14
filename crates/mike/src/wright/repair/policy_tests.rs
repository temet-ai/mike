use super::policy::{Category, ChangedFile, MAX_FILES, MAX_LINES, category, check, parse_numstat};

fn file(path: &str, added: u64, removed: u64) -> ChangedFile {
    ChangedFile {
        path: path.to_owned(),
        added: Some(added),
        removed: Some(removed),
    }
}

#[test]
fn repair_policy_refuses_every_protected_path_category() {
    let cases = [
        (
            "crates/mike/src/wright/repair/policy.rs",
            Category::WrightAuthority,
        ),
        (
            "crates/mike/src/wright/watch/mod.rs",
            Category::WrightAuthority,
        ),
        (
            "crates/mike/src/store/persistence/wright/repairs.rs",
            Category::WrightAuthority,
        ),
        (
            "crates/mike/src/api/wright/repairs.rs",
            Category::WrightAuthority,
        ),
        (
            "crates/mike/src/api/manifest/wright.rs",
            Category::WrightAuthority,
        ),
        (
            "crates/mike-base/src/store/persistence/migrations/20261115030000_wright_repair.sql",
            Category::WrightAuthority,
        ),
        (
            "crates/mike-base/src/feature_toggles/meta.rs",
            Category::WrightAuthority,
        ),
        (
            "crates/mike-base/src/web_base/host_guard/authority.rs",
            Category::Authentication,
        ),
        (
            "crates/mike-features/src/service_token.rs",
            Category::Authentication,
        ),
        ("crates/mike/src/api/auth.rs", Category::Authentication),
        (
            "crates/mike/src/api/wright_conversation/operator_token.rs",
            Category::WrightAuthority,
        ),
        (
            "crates/mike/src/conversation/operator_token.rs",
            Category::Authentication,
        ),
        ("scripts/pre-push-check", Category::GateConfig),
        ("scripts/test-area", Category::GateConfig),
        (".githooks/pre-push", Category::GateConfig),
        ("Makefile", Category::GateConfig),
        (
            ".claude/rules/rust-database-reliability.md",
            Category::GateConfig,
        ),
        (".github/workflows/rust-quality.yml", Category::GateConfig),
        (".semgrep/rules.yml", Category::GateConfig),
        ("deny.toml", Category::GateConfig),
        ("clippy.toml", Category::GateConfig),
        ("rustfmt.toml", Category::GateConfig),
        (".cargo/config.toml", Category::GateConfig),
        (".config/nextest.toml", Category::GateConfig),
        ("Cargo.toml", Category::GateConfig),
        ("crates/mike/Cargo.toml", Category::GateConfig),
        ("AGENTS.md", Category::GateConfig),
        (".env", Category::Credentials),
        (".env.local", Category::Credentials),
        ("crates/mike/src/secrets.rs", Category::Credentials),
        ("config/secrets/token.txt", Category::Credentials),
        (
            "crates/mike-base/src/config/credentials.rs",
            Category::Credentials,
        ),
        ("certs/server.pem", Category::Credentials),
        ("scripts/deploy-proxy.sh", Category::Deploy),
        (
            "scripts/systemd/mike-stack-proxy.service.in",
            Category::Deploy,
        ),
        ("scripts/start-proxy.sh", Category::Deploy),
        (".deploy/mike.deployed", Category::Deploy),
    ];
    for (path, expected) in cases {
        assert_eq!(category(path), Some(expected), "{path}");
        let refused = check(&[file(path, 1, 0)], "");
        assert!(refused.is_err(), "{path} must be refused");
    }
}

#[test]
fn repair_policy_allows_ordinary_code_docs_and_tests() {
    for path in [
        "crates/mike-features/src/proxy/handler.rs",
        "crates/mike-fleet/src/change_record/watch/author.rs",
        "crates/mike-lessons/src/lesson_proposer/skill_author.rs",
        "crates/mike/src/api/handlers/health/dto_tests.rs",
        "frontend/src/routes/sessions/+page.svelte",
        "docs/ARCHITECTURE.md",
        "src/calc.sh",
        ".sqlx/query-abc.json",
    ] {
        assert_eq!(category(path), None, "{path}");
    }
    assert_eq!(check(&[file("src/calc.sh", 3, 1)], ""), Ok(()));
}

#[test]
fn repair_policy_refuses_a_migration_that_names_wright_or_a_toggle() {
    let path = "crates/mike-base/src/store/persistence/migrations/20991231000000_tidy.sql";
    let toggles = "+++ b/x.sql\n+UPDATE config.feature_toggles SET enabled = true;\n";
    assert!(check(&[file(path, 1, 0)], toggles).is_err());
    let wright = "+DELETE FROM WRIGHT.repairs;\n";
    assert!(check(&[file(path, 1, 0)], wright).is_err());
    let removed_only = "-UPDATE config.feature_toggles SET enabled = true;\n+SELECT 1;\n";
    assert_eq!(check(&[file(path, 1, 1)], removed_only), Ok(()));
}

#[test]
fn repair_policy_refuses_binary_and_oversized_patches() {
    let binary = ChangedFile {
        path: "assets/logo.png".to_owned(),
        added: None,
        removed: None,
    };
    assert!(
        check(&[binary], "")
            .err()
            .unwrap_or_default()
            .contains("binary")
    );
    let too_many: Vec<ChangedFile> = (0..=MAX_FILES)
        .map(|n| file(&format!("src/f{n}.rs"), 1, 0))
        .collect();
    assert!(
        check(&too_many, "")
            .err()
            .unwrap_or_default()
            .contains("at most")
    );
    assert!(check(&[file("src/big.rs", MAX_LINES, 1)], "").is_err());
    assert_eq!(check(&[file("src/big.rs", MAX_LINES, 0)], ""), Ok(()));
}

#[test]
fn repair_numstat_parses_paths_counts_and_binaries() {
    let raw = "3\t1\tsrc/calc.sh\0-\t-\tassets/logo.png\x000\t5\tscripts/check.sh\0";
    let files = parse_numstat(raw);
    assert_eq!(
        files,
        vec![
            file("src/calc.sh", 3, 1),
            ChangedFile {
                path: "assets/logo.png".to_owned(),
                added: None,
                removed: None,
            },
            file("scripts/check.sh", 0, 5),
        ]
    );
}
