#!/usr/bin/env bash
# test_pipeline_sigpipe.sh — regression test for the pre-commit-hook.sh
# SIGPIPE bug (fleet-judgement-at-work, 2026-09-28): under `set -o pipefail`,
# `grep ... | head -N` on a grader's finding pipeline dies with a bare
# "grep: write error: Broken pipe" (exit 141) whenever the grader reports
# enough matching lines to make grep block on a write() that head has
# already stopped reading — hiding the real finding and looking like the
# hook crashed for no reason. pre-commit-hook.sh now uses `sed -n '1,Np'`
# instead of `head -N` for every finding-summary pipeline, which reads its
# whole input to completion and can never SIGPIPE its own source.
#
# A small fixture (10 lines) does not reproduce the race reliably — the OS
# pipe buffer (commonly 64KiB) can absorb it before head closes, so grep
# exits cleanly regardless of pattern. This test generates enough matching
# lines to exceed that buffer, which reproduces the crash deterministically
# with the old pattern and proves the new one survives it.
#
# Usage: scripts/db-code-graders/test_pipeline_sigpipe.sh
# Exit: 0 both cases behave as expected, 1 otherwise.
set -uo pipefail

tmp="$(mktemp)"
export tmp
trap 'rm -f "$tmp"' EXIT

python3 -c "
for i in range(20000):
    print(f'  L{i}: fake finding with enough padding text to force the pipe buffer to fill {i}')
" > "$tmp"

fail=0

old_pattern() {
    set -euo pipefail
    grep -E '^\s+L[0-9]+:' "$tmp" | head -3 | sed 's/^[[:space:]]*//'
}

new_pattern() {
    set -euo pipefail
    grep -E '^\s+L[0-9]+:' "$tmp" | sed -n '1,3p' | sed 's/^[[:space:]]*//'
}

echo "=== test_pipeline_sigpipe ==="

if out="$(bash -c "$(declare -f old_pattern); old_pattern" 2>&1)"; then
    echo "FAIL: old head -N pattern was expected to SIGPIPE on a >64KiB grep output but exited 0"
    echo "  (this would mean the OS pipe buffer changed size, or the repro fixture is too small)"
    fail=1
else
    echo "PASS: old head -N pattern reproduces the SIGPIPE crash (exit $?) — confirms the bug is real"
fi

if out="$(bash -c "$(declare -f new_pattern); new_pattern" 2>&1)"; then
    lines="$(printf '%s\n' "$out" | wc -l | tr -d ' ')"
    first="$(printf '%s\n' "$out" | head -1)"
    if [[ "$lines" -eq 3 && "$first" == "L0: fake finding with enough padding text to force the pipe buffer to fill 0" ]]; then
        echo "PASS: new sed -n pattern survives the same input, exit 0, correct 3-line truncation"
    else
        echo "FAIL: new sed -n pattern exited 0 but output was wrong: got $lines line(s), first='$first'"
        fail=1
    fi
else
    echo "FAIL: new sed -n pattern crashed (exit $?) on the same input that broke the old one"
    fail=1
fi

if grep -n 'head -[0-9]' "$(dirname "$0")/pre-commit-hook.sh" >/dev/null 2>&1; then
    echo "FAIL: pre-commit-hook.sh still contains a 'head -N' finding-pipeline pattern"
    grep -n 'head -[0-9]' "$(dirname "$0")/pre-commit-hook.sh"
    fail=1
else
    echo "PASS: pre-commit-hook.sh has no remaining head -N finding pipeline"
fi

if [[ "$fail" -eq 0 ]]; then
    echo "=== ALL PASS ==="
else
    echo "=== FAIL ==="
fi
exit "$fail"
