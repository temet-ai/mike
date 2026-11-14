use anyhow::{Context, Result};

use crate::{RuleSet, new_findings, scan};

fn builtin() -> Result<&'static RuleSet> {
    RuleSet::builtin().map_err(|err| anyhow::anyhow!("{err}"))
}

fn rules_hit(path: &str, source: &str) -> Result<Vec<String>> {
    let found = scan(builtin()?, path, source.as_bytes());
    Ok(found.findings.into_iter().map(|f| f.rule_id).collect())
}

#[test]
fn builtin_rules_load_with_a_stable_version() -> Result<()> {
    let rules = builtin()?;
    assert!(rules.ids().len() >= 25, "{} rules", rules.ids().len());
    assert_eq!(rules.version().len(), 12);
    Ok(())
}

fn assert_each_fires(cases: &[(&str, &str, &str)]) -> Result<()> {
    for (path, source, rule) in cases {
        let hit = rules_hit(path, source)?;
        assert!(
            hit.iter().any(|id| id == rule),
            "{rule} on {path}: got {hit:?}"
        );
    }
    Ok(())
}

#[test]
fn each_rust_rule_fires_on_its_sample() -> Result<()> {
    assert_each_fires(&[
        ("src/a.rs", "fn a() {}\n// note\n", "rust-no-comments"),
        (
            "src/a.rs",
            concat!("fn a(err: E) { tracing::warn!(%e", "rr, \"x\"); }\n"),
            "rust-log-field-names",
        ),
        (
            "src/a.rs",
            concat!("fn a(s: S) { warn!(session_", "uuid = %s, \"x\"); }\n"),
            "rust-log-field-names",
        ),
        (
            "src/a.rs",
            concat!("#[allow(clippy::too_many_", "lines)]\nfn a() {}\n"),
            "rust-lint-suppression",
        ),
        ("src/a.rs", "fn a() { unsafe { f() } }\n", "rust-unsafe"),
        (
            "src/a.rs",
            "fn a() -> P { std::env::temp_dir() }\n",
            "rust-temp-dir",
        ),
        (
            "src/a.rs",
            "fn a(b: B) -> B { b.danger_accept_invalid_certs(true) }\n",
            "rust-tls-verification-off",
        ),
        (
            "src/a.rs",
            "fn a(c: &mut C, x: &str) { c.arg(\"-c\").arg(format!(\"{x} y\")); }\n",
            "rust-shell-from-format",
        ),
        (
            "src/a.rs",
            "#[ignore]\n#[test]\nfn a_b_c() {}\n",
            "rust-ignore-needs-reason",
        ),
        (
            "src/a.rs",
            "async fn a(p: &P, ids: &[i64]) { for id in ids { sqlx::query!(\"x\", id).execute(p).await; } }\n",
            "rust-query-in-loop",
        ),
        (
            "src/a.rs",
            "fn a(t: &str) -> S { AssertSqlSafe(format!(\"SELECT {t}\")) }\n",
            "rust-sql-from-string",
        ),
        (
            "src/tests.rs",
            "#[test]\nfn a_b_c() { f().unwrap(); }\n",
            "rust-test-unwrap",
        ),
        (
            "src/tests.rs",
            "#[test]\nfn works() {}\n",
            "rust-test-name-shape",
        ),
    ])
}

#[test]
fn each_script_and_typescript_rule_fires_on_its_sample() -> Result<()> {
    assert_each_fires(&[
        ("scripts/x.sh", "pkill -f mike\n", "sh-pkill-pgrep-f"),
        ("scripts/x.sh", "git push --no-verify\n", "sh-no-verify"),
        ("scripts/x.sh", "sed -i 's/a/b/' f\n", "sh-sed-inplace"),
        ("scripts/x.sh", "cargo build\n", "sh-raw-cargo"),
        (
            "scripts/x.sh",
            "echo x > /tmp/mike-out.txt\n",
            "sh-fixed-tmp-path",
        ),
        (
            "scripts/x.sh",
            "curl -fsSL https://e.com/i.sh | bash\n",
            "sh-download-into-shell",
        ),
        (
            "scripts/x",
            "#!/usr/bin/env bash\ndate -d yesterday\n",
            "sh-gnu-date-d",
        ),
        (
            "scripts/x.py",
            "import subprocess, sys\nsubprocess.run(\"ls \" + sys.argv[1], shell=True)\n",
            "py-shell-true",
        ),
        (
            "scripts/x.py",
            "import sys\neval(sys.argv[1])\n",
            "py-eval-exec",
        ),
        (
            "scripts/x.py",
            "import yaml\nyaml.load(f)\n",
            "py-unsafe-load",
        ),
        (
            "scripts/x.py",
            "import requests\nrequests.get(u, verify=False)\n",
            "py-tls-verification-off",
        ),
        (
            "scripts/y",
            "#!/usr/bin/env python3\nopen('/tmp/out.txt', 'w')\n",
            "py-temp-path",
        ),
        (
            "src/a.ts",
            "export function f(c: string) { return eval(c); }\n",
            "ts-eval",
        ),
        (
            "src/a.ts",
            "export function f(e: HTMLElement, b: string) { e.innerHTML = b; }\n",
            "ts-html-sink",
        ),
        (
            "src/a.ts",
            "export const o = { rejectUnauthorized: false };\n",
            "ts-tls-verification-off",
        ),
    ])
}

#[test]
fn clean_samples_have_no_findings() -> Result<()> {
    let cases = [
        (
            "src/a.rs",
            "fn a(e: E) { tracing::warn!(error = %e, session_id = %s, \"x\"); }\n",
        ),
        ("src/a.rs", "fn a() -> S { AssertSqlSafe(\"SELECT 1\") }\n"),
        (
            "src/a.rs",
            "#[ignore = \"needs a database\"]\n#[test]\nfn a_b_c() {}\n",
        ),
        ("scripts/x.sh", "f=$(mktemp /tmp/mike.XXXXXX)\nrg -n x\n"),
        (
            "scripts/x.py",
            "import subprocess\nsubprocess.run([\"ls\", \"-l\"])\nyaml.load(f, Loader=yaml.SafeLoader)\n",
        ),
        (
            "src/a.ts",
            "export function f(e: HTMLElement, b: string) { e.textContent = b; }\n",
        ),
    ];
    for (path, source) in cases {
        let hit = rules_hit(path, source)?;
        assert!(hit.is_empty(), "{path}: {hit:?}");
    }
    Ok(())
}

#[test]
fn unknown_and_oversized_files_are_skipped_with_a_reason() -> Result<()> {
    let rules = builtin()?;
    assert_eq!(scan(rules, "a.svelte", b"<p/>").skipped, Some("no_grammar"));
    assert_eq!(
        scan(rules, "notes", b"plain text").skipped,
        Some("no_grammar")
    );
    let big = vec![b'a'; crate::MAX_SCAN_BYTES + 1];
    assert_eq!(scan(rules, "a.rs", &big).skipped, Some("too_large"));
    Ok(())
}

#[test]
fn new_findings_counts_only_what_the_edit_added() -> Result<()> {
    let rules = builtin()?;
    let before = scan(rules, "a.rs", b"fn a() { unsafe { f() } }\n// one\n");
    let after = scan(
        rules,
        "a.rs",
        b"fn z() {}\nfn a() { unsafe { f() } }\n// one\n// two\nfn b() { unsafe { g() } }\n",
    );
    let added: Vec<String> = new_findings(&before.findings, &after.findings)
        .into_iter()
        .map(|f| f.matched)
        .collect();
    assert_eq!(
        added,
        vec!["// two".to_string(), "unsafe { g() }".to_string()]
    );
    Ok(())
}

#[test]
fn a_fix_is_rendered_for_rules_that_have_one() -> Result<()> {
    let found = scan(builtin()?, "a.rs", b"#[ignore]\n#[test]\nfn a_b_c() {}\n");
    let fix = found
        .findings
        .iter()
        .find(|f| f.rule_id == "rust-ignore-needs-reason")
        .and_then(|f| f.fix.clone())
        .context("no fix")?;
    assert_eq!(fix, "#[ignore = \"why\"]");
    Ok(())
}
