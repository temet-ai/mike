"""The two defenses against clobbering a living doc with a NO_CHANGES stub:
the lenient sentinel (is_no_change) and the destructive-write guard (write)."""

import pytest

import client
import git_util
from steps import architecture, guides, ui_help

FULL_DOC = "\n".join(f"line {i} of a real document" for i in range(1, 301))


# ── is_no_change: lenient sentinel ─────────────────────────────────────────────


def test_decorated_sentinel_is_no_change():
    result = "NO_CHANGES\n\nThe diff adds X; the doc remains accurate."
    assert git_util.is_no_change(result) is True


def test_bare_sentinel_is_no_change():
    assert git_util.is_no_change("NO_CHANGES") is True


@pytest.mark.parametrize(
    "result",
    [
        "",
        "   \n  ",
        "NO_CHANGES\n",
        "**NO_CHANGES**",
        "`NO_CHANGES`",
        "no changes.",
        "No changes needed — the diff is test-only.",
        "NO CHANGES",
        "no_changes",
        "NO_CHANGES: only tests changed.",
    ],
)
def test_sentinel_variants_are_no_change(result):
    assert git_util.is_no_change(result) is True


@pytest.mark.parametrize(
    "result",
    [
        # Real document whose opening prose lexically resembles the sentinel.
        "No changes to the auth flow; however the port table below was updated.\n\n"
        + FULL_DOC,
        # Comma negation — changes ARE needed; must never match the sentinel.
        "No, changes are needed. Here is the doc:\n\n" + FULL_DOC,
        # Long prose reply: past the short-reply cap it's a document, and the
        # write guard — not the sentinel — is the defense against writing it.
        "No changes are needed for this diff.\n"
        + "\n".join(f"explanation line {i}" for i in range(1, 20)),
    ],
)
def test_sentinel_lookalikes_are_not_no_change(result):
    assert git_util.is_no_change(result) is False


def test_full_document_is_not_no_change():
    assert git_util.is_no_change(FULL_DOC) is False


def test_document_mentioning_sentinel_later_is_not_no_change():
    doc = "# Architecture\n\nThe agent replies NO_CHANGES when idle.\n" + FULL_DOC
    assert git_util.is_no_change(doc) is False


# ── write: destructive-write guard ─────────────────────────────────────────────


@pytest.fixture
def tmp_root(tmp_path, monkeypatch):
    monkeypatch.setattr(git_util, "ROOT", tmp_path)
    return tmp_path


def test_write_refuses_stub_over_existing_doc(tmp_root):
    existing = "\n".join(f"existing line {i}" for i in range(1, 101)) + "\n"
    (tmp_root / "docs").mkdir()
    (tmp_root / "docs/DOC.md").write_text(existing)

    with pytest.raises(git_util.SuspiciousWriteError):
        git_util.write("docs/DOC.md", "one\ntwo\nthree\n")

    assert (tmp_root / "docs/DOC.md").read_text() == existing


def test_write_accepts_full_document_over_existing_doc(tmp_root):
    (tmp_root / "docs").mkdir()
    (tmp_root / "docs/DOC.md").write_text(
        "\n".join(f"old line {i}" for i in range(1, 101))
    )

    git_util.write("docs/DOC.md", FULL_DOC)

    assert (tmp_root / "docs/DOC.md").read_text() == FULL_DOC


def test_write_accepts_new_file(tmp_root):
    git_util.write("docs/NEW.md", "short\n")
    assert (tmp_root / "docs/NEW.md").read_text() == "short\n"


def test_write_allows_shrinking_small_files(tmp_root):
    # ≤20-line files are below the guard's floor — legitimate small docs may shrink.
    (tmp_root / "small.md").write_text("a\nb\nc\nd\ne\n")
    git_util.write("small.md", "a\n")
    assert (tmp_root / "small.md").read_text() == "a\n"


# ── step-level: decorated sentinel must not reach the file ─────────────────────

CRATE_DIFF = """\
diff --git a/crates/mike/src/lib.rs b/crates/mike/src/lib.rs
index 1111111..2222222 100644
--- a/crates/mike/src/lib.rs
+++ b/crates/mike/src/lib.rs
@@ -1,2 +1,3 @@
 fn main() {}
+// new line
"""


@pytest.fixture
def arch_doc(tmp_root):
    doc = "# Architecture\n\n" + "\n".join(
        f"architecture fact {i}" for i in range(1, 40)
    )
    (tmp_root / "docs").mkdir()
    (tmp_root / "docs/ARCHITECTURE.md").write_text(doc)
    return doc


def test_step_skips_write_on_decorated_sentinel(arch_doc, tmp_root, monkeypatch):
    monkeypatch.setattr(
        client,
        "complete",
        lambda system, user, model=None: (
            "NO_CHANGES\n\nThe diff adds X; the doc remains accurate."
        ),
    )

    updated = architecture.run(CRATE_DIFF)

    assert updated == []
    assert (tmp_root / "docs/ARCHITECTURE.md").read_text() == arch_doc


def test_step_writes_full_document(arch_doc, tmp_root, monkeypatch):
    new_doc = "# Architecture\n\n" + "\n".join(
        f"updated architecture fact {i}" for i in range(1, 40)
    )
    monkeypatch.setattr(client, "complete", lambda system, user, model=None: new_doc)

    updated = architecture.run(CRATE_DIFF)

    assert updated == ["docs/ARCHITECTURE.md"]
    assert (tmp_root / "docs/ARCHITECTURE.md").read_text() == new_doc


def test_step_survives_guard_refusal(arch_doc, tmp_root, monkeypatch):
    # A stub that is NOT a sentinel: the guard is the last line of defense,
    # and a refusal must not crash the run.
    monkeypatch.setattr(
        client,
        "complete",
        lambda system, user, model=None: "This doc describes the proxy.\nAll good.\n",
    )

    updated = architecture.run(CRATE_DIFF)

    assert updated == []
    assert (tmp_root / "docs/ARCHITECTURE.md").read_text() == arch_doc


# ── step-level: the two docs restored by #125 that PR #106's restore missed ────
# (docs/development/sqlx-prepare.md, docs/development/mcp-from-monitor.md — both
# go through guides.run(), the same is_no_change()/write() guard as architecture.py)

TOOLING_DIFF = """\
diff --git a/scripts/start-proxy.sh b/scripts/start-proxy.sh
index 1111111..2222222 100755
--- a/scripts/start-proxy.sh
+++ b/scripts/start-proxy.sh
@@ -1,2 +1,3 @@
 #!/bin/bash
+# new line
"""


@pytest.fixture
def guide_docs(tmp_root):
    # Order matches _GUIDE_DOCS[1:] (sqlx-prepare.md, mcp-from-monitor.md);
    # TROUBLESHOOTING.md is deliberately absent so guides.run() skips it via
    # `if not content: continue` without consuming a client.complete() call.
    docs = {}
    for path in ("docs/development/sqlx-prepare.md", "docs/development/mcp-from-monitor.md"):
        doc = f"# {path}\n\n" + "\n".join(f"guide fact {i}" for i in range(1, 40))
        full = tmp_root / path
        full.parent.mkdir(parents=True, exist_ok=True)
        full.write_text(doc)
        docs[path] = doc
    return docs


def test_guides_step_skips_write_on_decorated_sentinel(guide_docs, tmp_root, monkeypatch):
    monkeypatch.setattr(
        client,
        "complete",
        lambda system, user, model=None: (
            "NO_CHANGES\n\nThe diff adds X; the doc remains accurate."
        ),
    )

    updated = guides.run(TOOLING_DIFF)

    assert updated == []
    for path, doc in guide_docs.items():
        assert (tmp_root / path).read_text() == doc


def test_guides_step_writes_full_document(guide_docs, tmp_root, monkeypatch):
    new_docs = {path: doc.replace("guide fact", "updated guide fact") for path, doc in guide_docs.items()}
    responses = iter(new_docs.values())
    monkeypatch.setattr(client, "complete", lambda system, user, model=None: next(responses))

    updated = guides.run(TOOLING_DIFF)

    assert set(updated) == set(guide_docs.keys())
    for path, doc in new_docs.items():
        assert (tmp_root / path).read_text() == doc


def test_guides_step_survives_guard_refusal(guide_docs, tmp_root, monkeypatch):
    monkeypatch.setattr(
        client,
        "complete",
        lambda system, user, model=None: "This doc describes the tooling.\nAll good.\n",
    )

    updated = guides.run(TOOLING_DIFF)

    assert updated == []
    for path, doc in guide_docs.items():
        assert (tmp_root / path).read_text() == doc


# ── step-level: docs/frontend/ROUTES.md, the third doc #125 had to restore ─────

ROUTES_DIFF = """\
diff --git a/frontend/src/routes/+page.svelte b/frontend/src/routes/+page.svelte
index 1111111..2222222 100644
--- a/frontend/src/routes/+page.svelte
+++ b/frontend/src/routes/+page.svelte
@@ -1,2 +1,3 @@
 <script></script>
+<!-- new -->
"""


@pytest.fixture
def routes_doc(tmp_root):
    doc = "# Routes\n\n" + "\n".join(f"route fact {i}" for i in range(1, 40))
    path = tmp_root / "docs/frontend/ROUTES.md"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(doc)
    return doc


def test_ui_help_step_skips_write_on_decorated_sentinel(routes_doc, tmp_root, monkeypatch):
    monkeypatch.setattr(
        client,
        "complete",
        lambda system, user, model=None: (
            "NO_CHANGES\n\nThe diff adds X; the doc remains accurate."
        ),
    )

    updated = ui_help.run(ROUTES_DIFF)

    assert updated == []
    assert (tmp_root / "docs/frontend/ROUTES.md").read_text() == routes_doc


def test_ui_help_step_writes_full_document(routes_doc, tmp_root, monkeypatch):
    new_doc = routes_doc.replace("route fact", "updated route fact")
    monkeypatch.setattr(client, "complete", lambda system, user, model=None: new_doc)

    updated = ui_help.run(ROUTES_DIFF)

    assert updated == ["docs/frontend/ROUTES.md"]
    assert (tmp_root / "docs/frontend/ROUTES.md").read_text() == new_doc


def test_guides_run_skips_one_doc_writes_another_divergent(guide_docs, tmp_root, monkeypatch):
    # guides.run() makes one client.complete() call per doc; a real run can see
    # NO_CHANGES for one doc and a rewrite for another in the SAME call — assert
    # each outcome lands independently rather than the whole run sharing a verdict.
    sqlx_path = "docs/development/sqlx-prepare.md"
    mcp_path = "docs/development/mcp-from-monitor.md"
    new_mcp_doc = guide_docs[mcp_path].replace("guide fact", "updated guide fact")

    def dispatching_complete(system, user, model=None):
        if f"Document: {sqlx_path}" in user:
            return "NO_CHANGES\n\nThe diff adds a comment; the doc remains accurate."
        assert f"Document: {mcp_path}" in user
        return new_mcp_doc

    monkeypatch.setattr(client, "complete", dispatching_complete)

    updated = guides.run(TOOLING_DIFF)

    assert sqlx_path not in updated
    assert mcp_path in updated
    assert (tmp_root / sqlx_path).read_text() == guide_docs[sqlx_path]
    assert (tmp_root / mcp_path).read_text() == new_mcp_doc


def test_ui_help_step_survives_guard_refusal(routes_doc, tmp_root, monkeypatch):
    monkeypatch.setattr(
        client,
        "complete",
        lambda system, user, model=None: "This doc describes the routes.\nAll good.\n",
    )

    updated = ui_help.run(ROUTES_DIFF)

    assert updated == []
    assert (tmp_root / "docs/frontend/ROUTES.md").read_text() == routes_doc
