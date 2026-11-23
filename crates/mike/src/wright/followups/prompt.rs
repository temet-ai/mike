const FIXER_PROMPT: &str = "\
You are Wright's fixer for {repo}#{number} (Wright follow-up {followup}).

1. Read the issue: `gh issue view {number} -R {repo} --comments`. Read the evidence it links: the problems and findings on the Wright page, the code, and the captured data they name.
2. Confirm the problem against the code and the live data before you change anything. For the code it names, answer: what it is trying to do and why it exists; what it should actually do; when it should run and which event should tell it to run; whether it belongs off the hot path; whether its queries read only what they use; and what test would fail if the problem came back.
3. If the issue is wrong, or the code needs no change, comment on the issue saying why, close it as not planned (`gh issue close {number} -R {repo} --reason \"not planned\"`), and end your session.
4. Make the fix those answers call for in this worktree, `{worktree}`, on its branch `{branch}`, and add the test that fails if the problem comes back. The branch is Wright's shared branch for the day: other fixes are committed on it. Do not create a worktree or a branch, and do not push, open a pull request or merge. Wright gates the branch once after the UTC day ends, opens one pull request for all the day's fixes and merges it.
5. Commit only your own paths (`git add <path>`, never `git add -A`) with a message that ends with these two lines: `Fixes #{number}` and `Wright-Followup: {followup}`. Run at most one `scripts/cx check --lib -p <crate>` for the crate you changed before you commit.
6. End your session once the commit is made or the issue is closed. The issue closes when the day's pull request merges.
";

pub(crate) struct FixerPlace<'a> {
    pub(crate) worktree: &'a str,
    pub(crate) branch: &'a str,
    pub(crate) followup_id: i64,
}

pub(crate) fn fixer_prompt(repo: &str, number: i32, place: &FixerPlace<'_>) -> String {
    FIXER_PROMPT
        .replace("{repo}", repo)
        .replace("{number}", &number.to_string())
        .replace("{followup}", &place.followup_id.to_string())
        .replace("{worktree}", place.worktree)
        .replace("{branch}", place.branch)
}
