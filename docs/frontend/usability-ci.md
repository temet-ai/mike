---
title: Automated UI Usability Review (CI)
status: active
created: 2026-06-28
updated: 2026-06-28
summary: How the ui-usability-review GitHub Actions workflow grades and gates frontend UI changes automatically using the usability skills, so usability checks run on every PR without manual agent invocation.
---

# Automated UI Usability Review (CI)

`.github/workflows/ui-usability-review.yml` runs on every PR that touches frontend UI source
(`frontend/src/**/*.{svelte,ts,css}`) and on changes to the usability skills or the gate
script. It applies the `usability-engineer` agent's four skills automatically — no manual
agent invocation.

## Why two tiers

An LLM grade alone is too noisy to block a build on, and a pure grep gate can't judge whether
copy actually reads clearly. So the workflow splits the job by what each tool is good at:

| Tier | Job | What it does | Blocks merge? | Cost |
|---|---|---|---|---|
| 1 | `gate` | Deterministic, API-free: the skills' executable self-checks (truncation, Tailwind `bg-[--foo]` miscompile) + `svelte-check` + the consolidated `frontendArchitecture.guard.test.ts`. | **Yes** — these are binding repo rules. | Free |
| 2 | `grade` | Claude Code (headless), **read-only**, grades the diff against the four skills and posts a sticky PR comment with a verdict + `file:line` findings. | **No** (advisory by default) | API tokens |

The principle: machines gate on the unambiguous, binding violations; the model advises on the
judgment calls (is this label clear? is help at the right tier?). You stay the decision-maker
on the advisory tier.

## One-time setup

Add an **`ANTHROPIC_API_KEY`** repository secret (Settings → Secrets and variables → Actions →
New repository secret). The Tier-1 `gate` job runs without it; the Tier-2 `grade` job is
skipped (with a notice) until the key is present.

> Using a Claude subscription instead of an API key? `anthropics/claude-code-action` also
> accepts `claude_code_oauth_token`. Swap the `anthropic_api_key:` input for
> `claude_code_oauth_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}` and add that secret.

## The deterministic gate (`scripts/usability-gate.sh`)

Runs the grep self-checks from the four skills over the **added** lines of the PR diff,
restricted to `frontend/src`. Two severities:

- **ERROR (fails the build):** content truncation on an inspection surface
  (`inspection-fidelity`), Tailwind `bg-[--foo]`/`text-[--foo]` bracket-only var
  (`tailwind-v4-pitfalls`).
- **WARN (advisory, never fails):** generic button labels (`Submit`/`OK`/…), raw status
  codes in copy, empty states that say nothing, filler/marketing words, native `title=`
  attributes, possible first-load tour auto-launch.

To run it locally:

```bash
scripts/usability-gate.sh --base main          # diff working branch against main
scripts/usability-gate.sh --base main --summary /tmp/report.md
```

## Making the grade gating (optional)

The `grade` job is advisory (`continue-on-error: true`) so a model verdict never blocks a
merge. To make it gating: set `continue-on-error: false` on the job and extend the prompt to
exit non-zero on a `NEEDS WORK` verdict. Recommended only once you trust the grade on your
codebase — start advisory, read a few PRs' worth of comments, then tighten.

## Optional: on-demand auto-fix (`@claude`)

The CI above **reviews**; it never edits. To let yourself trigger the `usability-engineer` to
actually apply fixes and push from a PR comment, add a second mention-triggered workflow:

```yaml
# .github/workflows/claude.yml
name: Claude
on:
  issue_comment:
    types: [created]
permissions:
  contents: write
  pull-requests: write
jobs:
  claude:
    if: contains(github.event.comment.body, '@claude')
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }
      - uses: anthropics/claude-code-action@v1
        with:
          anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}
          # Comment "@claude run the usability-engineer on the changed pages and push the fixes"
```

Then comment `@claude run the usability-engineer on this PR and push the usability fixes`. It
runs the editor agent (which obeys the same skills + binding rules and verifies its work).
This is kept opt-in rather than automatic because it pushes commits — you decide when.

## What it does NOT cover

The grade reasons over source, not pixels. It cannot see rendered contrast, real layout shift
at runtime, or screen-reader output. For those, the `usability-engineer` agent's verification
step (live Chrome MCP + `scripts/sb-test`) is still the bar before a usability change is called
done. CI catches regressions in the source-visible rules; live verification catches the rest.
