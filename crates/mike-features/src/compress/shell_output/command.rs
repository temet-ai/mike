#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandShape {
    Read,
    Listing,
    Other,
}

const READ_PROGRAMS: [&str; 9] = [
    "sed", "cat", "head", "tail", "bat", "nl", "less", "jq", "awk",
];
const LISTING_PROGRAMS: [&str; 10] = [
    "rg", "grep", "ls", "fd", "find", "wc", "tree", "du", "eza", "stat",
];
const GIT_READS: [&str; 3] = ["show", "diff", "blame"];
const GIT_LISTINGS: [&str; 6] = ["status", "worktree", "log", "branch", "ls-files", "grep"];

const SUBCOMMAND_PROGRAMS: [&str; 10] = [
    "cargo",
    "cx",
    "git",
    "gh",
    "make",
    "npm",
    "pnpm",
    "docker",
    "mike",
    "systemctl",
];
const SKIPPED_STARTS: [&str; 12] = [
    "cd", "for", "while", "if", "done", "fi", "else", "set", "echo", "export", "}", "printf",
];
const CONTROL_PREFIXES: [&str; 4] = ["do", "then", "else", "{"];

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn program_words(segment: &str) -> Option<(&str, Option<&str>)> {
    let first = segment.split('|').next()?;
    let mut words = first
        .split_whitespace()
        .map(|word| word.trim_start_matches('('))
        .skip_while(|word| CONTROL_PREFIXES.contains(word))
        .skip_while(|word| is_assignment(word) || *word == "env");
    let program = words.next()?;
    if SKIPPED_STARTS.contains(&program) || program.ends_with("(){") || program.is_empty() {
        return None;
    }
    let program = program.rsplit('/').next().unwrap_or(program);
    Some((program, words.find(|word| !word.starts_with('-'))))
}

fn first_program(command: &str) -> (&str, Option<&str>) {
    command
        .split(['\n', ';'])
        .flat_map(|line| line.split("&&"))
        .flat_map(|part| part.split("||"))
        .map(str::trim)
        .find_map(program_words)
        .unwrap_or(("", None))
}

pub fn command_kind(command: &str) -> String {
    match first_program(command) {
        ("", _) => "(empty)".to_string(),
        (program, Some(sub)) if SUBCOMMAND_PROGRAMS.contains(&program) => {
            format!("{program} {sub}")
        }
        (program, _) => program.to_string(),
    }
}

pub fn command_shape(command: &str) -> CommandShape {
    let (program, sub) = first_program(command);
    if READ_PROGRAMS.contains(&program) {
        return CommandShape::Read;
    }
    if LISTING_PROGRAMS.contains(&program) {
        return CommandShape::Listing;
    }
    if program == "git" {
        let sub = sub.unwrap_or("");
        if GIT_READS.contains(&sub) {
            return CommandShape::Read;
        }
        if GIT_LISTINGS.contains(&sub) {
            return CommandShape::Listing;
        }
    }
    CommandShape::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_shape_sorts_reads_listings_and_the_rest() {
        for (command, shape) in [
            ("sed -n '120,180p' src/lib.rs", CommandShape::Read),
            ("cd /repo && cat Cargo.toml", CommandShape::Read),
            ("git --no-pager show HEAD:src/a.rs", CommandShape::Read),
            ("rg -n foo src | head -50", CommandShape::Listing),
            ("ls -la crates", CommandShape::Listing),
            ("git status --short", CommandShape::Listing),
            ("cargo test -p mike 2>&1 | grep FAIL", CommandShape::Other),
            ("FOO=1 scripts/cx nextest run", CommandShape::Other),
            ("git fetch origin", CommandShape::Other),
            ("", CommandShape::Other),
            ("S=/tmp/x; cat $S/out.txt", CommandShape::Read),
            (
                "cd /repo && S=/tmp/x && rg -n foo $S",
                CommandShape::Listing,
            ),
            (
                "for f in a b; do sed -n '1,9p' $f; done",
                CommandShape::Read,
            ),
            (
                "for f in *.rs\ndo\n  wc -l \"$f\"\ndone",
                CommandShape::Listing,
            ),
            ("Q(){ psql -c \"$1\"; }; Q 'select 1'", CommandShape::Other),
        ] {
            assert_eq!(command_shape(command), shape, "{command}");
        }
    }

    #[test]
    fn command_kind_names_the_program_past_assignments_and_loops() {
        for (command, kind) in [
            ("rg -n foo src", "rg"),
            ("cd /repo && cargo test -p mike", "cargo test"),
            ("cd /repo; git --no-pager log --oneline", "git log"),
            ("FOO=1 scripts/cx nextest run", "cx nextest"),
            ("S=/tmp/x; ON=$(uuidgen); tmux new-session -d", "tmux"),
            ("for f in a b; do cat $f; done", "cat"),
            ("echo ===ON; tail -5 log; echo ===OFF", "tail"),
            ("export X=1; }; git status", "git status"),
            ("cd x", "(empty)"),
            ("", "(empty)"),
        ] {
            assert_eq!(command_kind(command), kind, "{command}");
        }
    }
}
