use super::*;

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
