---
title: Rust Comment Conventions — Extended Rationale & Citations
status: active
created: 2026-07-01
updated: 2026-07-01
summary: Extended why-this-rule-exists narrative and the external citations supporting the Rust comment conventions rule, relocated out of the always-loaded rule body.
---

The rule is graded by `scripts/rule-checks/rust-comments.py` and
`scripts/rule-checks/rust-no-comments.sh`. This doc holds the extended rationale and
supporting citations, relocated verbatim to keep the rule body lean.

## Why this rule exists

The recurring failure: comments narrate the **history** of the code instead of
documenting the code **as it is**. The operator was reading
`crates/mike/src/store/persistence/mod.rs` and found more change-narrative
than code — a three-line function buried under four lines of "called by X after
the remote DB was unreachable", a six-line block describing a `persistence_test_lock()`
helper that *no longer exists*, a resolver block opening with "The migration squash
collapsed every `<parent>_ref`…". At the time of writing there were ~151 such
history-narration comment-lines across 63 files.

Every one of those lines rots the moment the history moves on, and a stale
historical comment is worse than no comment: it is a false contract the next
reader builds on. The history is **already recorded** — in `git log` / `git blame`
(per-line authorship, exact date, the full commit message — "a whole novel" if it
needs to be) and in the capture DB. Source comments are not a
second, hand-maintained, always-drifting copy of that record.

> "Those comments belong in the version control commit comments. Version control
> should only contain the live code." — *Comments are not Version Control*
> (Abel Avram). Echoed by the Linux kernel coding style ("who wrote a specific
> function on a specific date … useless things") and *A Philosophy of Software
> Design* / *Clean Code* (stale comments "spread lies about the code and mislead
> the reader").

## Supporting citations

These annotate specific clauses in the rule body:

- **Doc vs implementation axis** — `///`/`//!` are rendered by rustdoc as the
  public contract; `//` is maintainer-only and never rendered.
  ([rustdoc book](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html),
  [RFC 1574](https://rust-lang.github.io/rfcs/1574-more-api-documentation-conventions.html))
- **Why, not what axis** — a comment carries the rationale the code cannot
  express; it does not restate the mechanics.
  ([The Rust Book ch.3.4](https://doc.rust-lang.org/book/ch03-04-comments.html),
  [Linux kernel coding style §8](https://docs.kernel.org/process/coding-style.html):
  "tell people what it does and possibly WHY it does it" — not HOW.)
- **Restating the signature (banned)** — rustdoc already hyperlinks the
  types; don't re-narrate them in prose.
  ([rustdoc book](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html))
- **`# Errors` / `# Panics` / `# Safety` / `# Examples` (keep)** —
  ([API Guidelines C-FAILURE / C-EXAMPLE](https://rust-lang.github.io/api-guidelines/documentation.html);
  Clippy `missing_errors_doc` / `missing_panics_doc` enforce the first two.)
- **Doc-comment summary-line form** —
  ([RFC 1574](https://rust-lang.github.io/rfcs/1574-more-api-documentation-conventions.html))
- **Don't document trivial private fns** —
  ([API Guidelines C-HIDDEN](https://rust-lang.github.io/api-guidelines/documentation.html))
