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

fn first_program(command: &str) -> (&str, Option<&str>) {
    let segment = command
        .split("&&")
        .flat_map(|part| part.split(';'))
        .map(str::trim)
        .find(|part| !part.is_empty() && !part.starts_with("cd "))
        .unwrap_or("");
    let first = segment.split('|').next().unwrap_or("");
    let mut words = first
        .split_whitespace()
        .skip_while(|word| word.contains('=') || *word == "env");
    let program = words.next().unwrap_or("");
    let program = program.rsplit('/').next().unwrap_or(program);
    (program, words.find(|word| !word.starts_with('-')))
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
        ] {
            assert_eq!(command_shape(command), shape, "{command}");
        }
    }
}
