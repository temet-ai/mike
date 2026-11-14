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
    let comments = rules
        .catalogue()
        .iter()
        .find(|r| r.id == "rust-no-comments")
        .context("rust-no-comments missing")?;
    assert_eq!(comments.language, "rust");
    assert!(
        comments
            .replaces
            .as_deref()
            .is_some_and(|u| u.contains("rust-database-reliability"))
    );
    Ok(())
}

fn assert_each_fires(cases: &[(&str, &str, &str)]) -> Result<()> {
    let mut missed = Vec::new();
    for (path, source, rule) in cases {
        let hit = rules_hit(path, source)?;
        if !hit.iter().any(|id| id == rule) {
            missed.push(format!("{rule} on {path}: got {hit:?}"));
        }
    }
    assert!(missed.is_empty(), "{missed:#?}");
    Ok(())
}

fn hits_rule(source: &str, rule: &str) -> Result<bool> {
    Ok(rules_hit("src/a.rs", source)?.iter().any(|id| id == rule))
}

#[test]
fn argument_and_file_size_rules_match_the_push_limits() -> Result<()> {
    let seven = "fn a(a: A, b: B, c: C, d: D, e: E, f: F, g: G) {}\n";
    assert!(hits_rule(seven, "rust-too-many-arguments")?);
    assert!(!hits_rule(
        "fn a(a: A, b: B, c: C, d: D, e: E, f: F) {}\n",
        "rust-too-many-arguments"
    )?);
    let in_trait_impl = format!("impl T for S {{\n    {seven}}}\n");
    assert!(!hits_rule(&in_trait_impl, "rust-too-many-arguments")?);
    assert!(hits_rule(&"fn a() {}\n".repeat(501), "rust-file-too-long")?);
    assert!(!hits_rule(
        &"fn a() {}\n".repeat(500),
        "rust-file-too-long"
    )?);
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

#[test]
fn each_systems_language_rule_fires_on_its_sample() -> Result<()> {
    assert_each_fires(&[
        (
            "main.go",
            "package main\nfunc f() { _ = &tls.Config{InsecureSkipVerify: true} }\n",
            "go-tls-verification-off",
        ),
        (
            "main.go",
            "package main\nfunc f(c string) { exec.Command(\"sh\", \"-c\", c).Run() }\n",
            "go-shell-command",
        ),
        (
            "a.c",
            "void f(char *s) { char b[8]; strcpy(b, s); }\n",
            "c-unsafe-string-call",
        ),
        (
            "a.c",
            "void f(char *s) { system(s); }\n",
            "c-system-from-value",
        ),
        (
            "a.cpp",
            "void f(char *s) { char b[8]; sprintf(b, \"%s\", s); }\n",
            "cpp-unsafe-string-call",
        ),
        (
            "a.cpp",
            "void f(char *s) { system(s); }\n",
            "cpp-system-from-value",
        ),
        (
            "A.java",
            "class A { void f(String c) throws Exception { Runtime.getRuntime().exec(c); } }\n",
            "java-exec-from-value",
        ),
        (
            "a.kt",
            "fun f(c: String) { Runtime.getRuntime().exec(c) }\n",
            "kotlin-exec-from-value",
        ),
        (
            "a.lua",
            "local function f(c) os.execute(c) end\n",
            "lua-run-from-value",
        ),
        ("a.rb", "def f(c)\n  eval(c)\nend\n", "ruby-eval"),
        (
            "a.rb",
            "def f(c)\n  system(\"ls #{c}\")\nend\n",
            "ruby-shell-interpolation",
        ),
        (
            "a.swift",
            "func f() { let x = try! g() }\n",
            "swift-force-try",
        ),
        (
            "a.ex",
            "defmodule A do\n  def f(c), do: Code.eval_string(c)\nend\n",
            "elixir-eval-string",
        ),
        (
            "a.ex",
            "defmodule A do\n  def f(c), do: String.to_atom(c)\nend\n",
            "elixir-atom-from-value",
        ),
        (
            "a.zig",
            "fn f() void { const x = g() catch unreachable; _ = x; }\n",
            "zig-catch-unreachable",
        ),
    ])
}

#[test]
fn systems_language_clean_samples_have_no_findings() -> Result<()> {
    let cases = [
        (
            "main.go",
            "package main\nfunc f() { exec.Command(\"ls\", \"-l\").Run() }\n",
        ),
        ("a.c", "void f(void) { system(\"ls\"); }\n"),
        ("a.lua", "local function f() os.execute(\"ls\") end\n"),
        ("a.rb", "def f(d)\n  system(\"ls\", d)\nend\n"),
        ("a.swift", "func f() { let y = try? g() }\n"),
        (
            "a.ex",
            "defmodule A do\n  def f(c), do: String.to_existing_atom(c)\nend\n",
        ),
    ];
    for (path, source) in cases {
        let hit = rules_hit(path, source)?;
        assert!(hit.is_empty(), "{path}: {hit:?}");
    }
    Ok(())
}

#[test]
fn languages_are_named_by_extension_and_shebang() {
    let cases = [
        ("a.go", "", Some("go")),
        ("a.kt", "", Some("kotlin")),
        ("a.hpp", "", Some("cpp")),
        ("a.zig", "", Some("zig")),
        ("a.exs", "", Some("elixir")),
        ("a.js", "", Some("typescript")),
        ("bin/x", "#!/usr/bin/env ruby\n", Some("ruby")),
        ("bin/x", "#!/usr/bin/env bun\n", Some("typescript")),
        ("a.ml", "", None),
        ("a.gleam", "", None),
        ("a.nim", "", None),
    ];
    for (path, head, expected) in cases {
        let got = crate::language_for(path, head.as_bytes()).map(crate::language_name);
        assert_eq!(got.as_deref(), expected, "{path}");
    }
    assert!(crate::grammars().len() >= 15);
}

#[test]
fn project_rules_join_the_builtin_set_and_are_named_project() -> Result<()> {
    let project = vec![(
        "rules/no-println.yml".to_string(),
        "id: no-println\nlanguage: rust\nseverity: warning\nrule:\n  pattern: println!($$$)\nmessage: Use tracing.\n"
            .to_string(),
    )];
    let rules = RuleSet::with_project(&project).map_err(|err| anyhow::anyhow!("{err}"))?;
    assert_ne!(rules.version(), builtin()?.version());
    let found = scan(&rules, "src/a.rs", b"fn a() { println!(\"x\"); }\n");
    let hit = found
        .findings
        .iter()
        .find(|f| f.rule_id == "no-println")
        .context("project rule did not fire")?;
    assert_eq!(hit.source, crate::SOURCE_PROJECT);
    assert!(
        found
            .findings
            .iter()
            .filter(|f| f.rule_id != "no-println")
            .all(|f| f.source == crate::SOURCE_BUILTIN)
    );
    Ok(())
}

#[test]
fn a_broken_project_rule_file_is_an_error_naming_the_file() {
    let project = vec![("rules/bad.yml".to_string(), "id: x\nrule: [".to_string())];
    let err = RuleSet::with_project(&project).err().map(|e| e.to_string());
    assert!(err.is_some_and(|e| e.contains("rules/bad.yml")));
}

#[test]
fn sgconfig_rule_dirs_are_read() -> Result<()> {
    let dirs = crate::sgconfig_rule_dirs("ruleDirs:\n  - rules\n  - lint/ast\n")
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    assert_eq!(dirs, vec!["rules".to_string(), "lint/ast".to_string()]);
    Ok(())
}
