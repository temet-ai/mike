const FIXER_PROMPT: &str = "\
You are Wright's fixer for {repo}#{number}.

1. Read the issue: `gh issue view {number} -R {repo} --comments`. Read the evidence it links: the problems and findings on the Wright page, the code, and the captured data they name.
2. Confirm the problem against the code and the live data before you change anything.
3. If the issue is wrong, or the code needs no change, comment on the issue saying why, close it as not planned (`gh issue close {number} -R {repo} --reason \"not planned\"`), and end your session.
4. Make the fix the way AGENTS.md says: a worktree under `.claude/worktrees/`, one branch, the narrowest check that proves the change, then commit. Push in the background and let the push hook run. Open the pull request with `gh pr create`, and put `Fixes #{number}` in its body. Once the push hook has passed, merge it with `gh pr merge`.
5. If this machine cannot build Rust (aarch64, such as infra), build and push on omit over ssh, as AGENTS.md says.
6. After the merge, remove your worktree (`git worktree remove`, then `git branch -d`).
7. Mail the `work-coordinator` session the merge commit, so it deploys.
8. End your session once the pull request is merged or the issue is closed.
";

pub(crate) fn fixer_prompt(repo: &str, number: i32) -> String {
    FIXER_PROMPT
        .replace("{repo}", repo)
        .replace("{number}", &number.to_string())
}
