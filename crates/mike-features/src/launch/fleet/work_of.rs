const WRAPPERS: [&str; 6] = ["command", "time", "nice", "exec", "nohup", "sudo"];
const LEADING_KEYWORDS: [&str; 8] = ["do", "then", "else", "elif", "if", "while", "until", "!"];
const INLINE_FLAGS: [&str; 3] = ["-", "-c", "-e"];
const INTERPRETERS: [&str; 7] = ["python3", "python", "node", "ruby", "perl", "bash", "sh"];
const SHELL_WORK: &[&str] = &[
    "cat", "echo", "printf", "sed", "awk", "grep", "rg", "egrep", "fgrep", "head", "tail", "cut",
    "sort", "uniq", "wc", "ls", "cd", "pwd", "date", "sleep", "free", "true", "false", "test", "[",
    "[[", "mkdir", "cp", "mv", "rm", "touch", "chmod", "tr", "tee", "xargs", "find", "which",
    "env", "export", "set", "unset", "source", ".", "for", "done", "fi", "case", "esac", "pgrep",
    "ps", "kill", "wait", "jq", "git", "read", "break", "continue", "return", "exit", "local",
    "declare", "shift", "trap", "type", "ulimit", "umask", "alias", "select", "function", "in",
    ":", "mike",
];

pub fn work_of(command: &str) -> Vec<String> {
    work_through(command, &[])
}

pub fn work_through(command: &str, wrappers: &[String]) -> Vec<String> {
    let mut found: Vec<String> = segments(&without_heredocs(command))
        .into_iter()
        .filter_map(|segment| work(&segment, wrappers))
        .collect();
    found.dedup();
    found
}

const FILE_WRITERS: [&str; 9] = [
    "tee", "mv", "cp", "rm", "touch", "mkdir", "chmod", "patch", "dd",
];

pub fn through_wrapper(command: &str, wrappers: &[String]) -> bool {
    segments(&without_heredocs(command)).iter().any(|segment| {
        let first = ungrouped(segment)
            .split_whitespace()
            .filter(|w| !redirect(w))
            .find(|w| !assignment(w) && !WRAPPERS.contains(w) && !LEADING_KEYWORDS.contains(w));
        first.is_some_and(|word| wrappers.iter().any(|w| is_wrapper(w, word)))
    })
}

pub fn edits_files(command: &str) -> bool {
    without_heredocs(command) != command || segments(command).iter().any(|s| writes_file(s))
}

fn writes_file(segment: &str) -> bool {
    let words: Vec<&str> = ungrouped(segment).split_whitespace().collect();
    let Some(at) = words
        .iter()
        .position(|w| !assignment(w) && !WRAPPERS.contains(w))
    else {
        return false;
    };
    let program = program_name(words[at]);
    if FILE_WRITERS.contains(&program) {
        return true;
    }
    if program == "sed" {
        return words.iter().any(|w| w.starts_with("-i"));
    }
    inline_script(program, words.get(at + 1).copied())
}

fn heredoc_end(line: &str) -> Option<String> {
    let at = line.find("<<")?;
    let rest = &line[at + 2..];
    if rest.starts_with('<') {
        return None;
    }
    let word: String = rest
        .trim_start_matches('-')
        .trim_start()
        .chars()
        .take_while(|c| !c.is_whitespace() && !matches!(c, ';' | '|' | '&' | ')' | '>'))
        .filter(|c| !matches!(c, '\'' | '"' | '\\'))
        .collect();
    (!word.is_empty()).then_some(word)
}

fn without_heredocs(command: &str) -> String {
    let mut kept = Vec::new();
    let mut until: Option<String> = None;
    for line in command.lines() {
        match &until {
            Some(end) if line.trim() == end => until = None,
            Some(_) => {}
            None => match heredoc_end(line) {
                Some(end) => until = Some(end),
                None => kept.push(line),
            },
        }
    }
    kept.join("\n")
}

#[derive(Default)]
struct Scan {
    parts: Vec<String>,
    current: String,
    quote: Option<char>,
}

impl Scan {
    fn cut(&mut self) {
        self.parts.push(std::mem::take(&mut self.current));
    }

    fn quoted(&mut self, quote: char, c: char, rest: &mut std::iter::Peekable<std::str::Chars>) {
        self.current.push(c);
        if c == quote {
            self.quote = None;
        } else if c == '\\' && quote == '"' {
            self.current.extend(rest.next());
        }
    }

    fn bare(&mut self, c: char, rest: &mut std::iter::Peekable<std::str::Chars>) {
        match c {
            '\'' | '"' => {
                self.quote = Some(c);
                self.current.push(c);
            }
            '\\' => {
                self.current.push(c);
                self.current.extend(rest.next());
            }
            ';' | '\n' => self.cut(),
            '|' => {
                rest.next_if_eq(&'|');
                self.cut();
            }
            '&' if rest.next_if_eq(&'&').is_some() => self.cut(),
            '&' if self.current.ends_with('>') || rest.peek() == Some(&'>') => {
                self.current.push(c);
            }
            '&' => self.cut(),
            _ => self.current.push(c),
        }
    }
}

fn segments(body: &str) -> Vec<String> {
    let mut scan = Scan::default();
    let mut rest = body.chars().peekable();
    while let Some(c) = rest.next() {
        match scan.quote {
            Some(quote) => scan.quoted(quote, c, &mut rest),
            None => scan.bare(c, &mut rest),
        }
    }
    scan.cut();
    scan.parts
}

fn ungrouped(segment: &str) -> &str {
    segment
        .trim()
        .trim_start_matches(['(', '{', ' '])
        .trim_end_matches([')', '}', ' '])
}

fn work(segment: &str, wrappers: &[String]) -> Option<String> {
    let cleaned = ungrouped(segment);
    let mut words: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|w| !redirect(w))
        .collect();
    strip_redirect_targets(&mut words, cleaned);
    while let Some(first) = words.first().copied() {
        if assignment(first)
            || WRAPPERS.contains(&first)
            || LEADING_KEYWORDS.contains(&first)
            || wrappers.iter().any(|w| is_wrapper(w, first))
        {
            words.remove(0);
        } else if first == "timeout" {
            words.drain(..words.len().min(2));
        } else {
            break;
        }
    }
    let program = program_name(words.first()?);
    if SHELL_WORK.contains(&program) || inline_script(program, words.get(1).copied()) {
        return None;
    }
    words[0] = program;
    Some(words.join(" "))
}

fn is_wrapper(wrapper: &str, word: &str) -> bool {
    word == wrapper
        || word
            .strip_suffix(wrapper)
            .is_some_and(|before| before.ends_with('/'))
}

fn strip_redirect_targets(words: &mut Vec<&str>, cleaned: &str) {
    let all: Vec<&str> = cleaned.split_whitespace().collect();
    let targets: Vec<&str> = all
        .windows(2)
        .filter(|pair| bare_redirect(pair[0]))
        .map(|pair| pair[1])
        .collect();
    words.retain(|w| !targets.contains(w));
}

fn bare_redirect(word: &str) -> bool {
    matches!(word, ">" | ">>" | "<" | "2>" | "2>>" | "&>" | "1>")
}

fn redirect(word: &str) -> bool {
    bare_redirect(word)
        || word == "2>&1"
        || word == "&"
        || ["2>", ">", "&>", "1>", "<"]
            .iter()
            .any(|prefix| word.starts_with(prefix) && word.len() > prefix.len())
}

fn assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn program_name(word: &str) -> &str {
    if word.starts_with("./") || word.contains('/') && !word.starts_with('/') {
        return word;
    }
    word.rsplit('/').next().unwrap_or(word)
}

fn inline_script(program: &str, next: Option<&str>) -> bool {
    INTERPRETERS.contains(&program) && next.is_none_or(|flag| INLINE_FLAGS.contains(&flag))
}

#[cfg(test)]
pub mod tests {
    use super::{edits_files, through_wrapper, work_of, work_through};

    #[test]
    fn a_command_through_a_wrapper_is_told_from_one_that_only_names_it() {
        let wrappers = vec!["scripts/builder-slot".to_string()];
        assert!(through_wrapper(
            "cd /repo && scripts/builder-slot builder check -p sim 2>&1 | tail",
            &wrappers
        ));
        assert!(through_wrapper(
            "FOO=1 /repo/scripts/builder-slot builder test",
            &wrappers
        ));
        assert!(!through_wrapper(
            "cat scripts/builder-slot; builder check",
            &wrappers
        ));
        assert!(!through_wrapper("builder check", &[]));
    }

    #[test]
    fn a_command_that_writes_a_file_before_its_work_edits() {
        for command in [
            "python3 - <<'EOF'
open('a.rs','w')
EOF
builder check -p sim",
            "sed -i 's/a/b/' x.rs && builder check",
            "mv a.rs b.rs; builder check",
            "python3 -c 'open(1)' && builder check",
        ] {
            assert!(edits_files(command), "{command}");
        }
        for command in [
            "cd /repo && builder check -p sim 2>&1 | tail -5",
            "builder nextest run -p sim > /tmp/out.log 2>&1; sed -n 1,5p /tmp/out.log",
            "python3 scripts/build.py --all",
        ] {
            assert!(!edits_files(command), "{command}");
        }
    }

    #[test]
    fn the_work_is_what_is_left_when_the_wrapping_is_taken_off() {
        assert_eq!(
            work_of(
                "cd /repo && export DATABASE_URL=x && scripts/cx check -p mike --lib 2>&1 | tail -20"
            ),
            vec!["scripts/cx check -p mike --lib"]
        );
        assert_eq!(
            work_of(
                "cd /repo && (timeout 580 scripts/cx check -p mike --lib 2>&1 | grep -E '^error')"
            ),
            vec!["scripts/cx check -p mike --lib"]
        );
        assert_eq!(
            work_of(
                "RUST_LOG=debug  builder   test -p core > /tmp/out.log 2>&1; tail /tmp/out.log"
            ),
            vec!["builder test -p core"]
        );
        assert_eq!(
            work_of("npm run build && npm test"),
            vec!["npm run build", "npm test"]
        );
        assert_eq!(work_of("/usr/bin/make check"), vec!["make check"]);
        assert_eq!(work_of("./check.sh"), vec!["./check.sh"]);
    }

    #[test]
    fn shell_utilities_git_and_inline_scripts_are_not_work() {
        for command in [
            "cd /repo && cat a.rs; sleep 60",
            "git add a && git commit -m x",
            "python3 - <<'EOF'\nimport os\nos.system('make test')\nEOF",
            "for i in 1 2 3; do sleep 5; done",
            "python3 -c 'print(1)'",
            "bash",
        ] {
            assert!(work_of(command).is_empty(), "{command}");
        }
        assert_eq!(
            work_of("python3 scripts/build.py --all"),
            vec!["python3 scripts/build.py --all"]
        );
    }

    #[test]
    fn a_pattern_in_quotes_is_not_cut_into_programs() {
        assert_eq!(
            work_of(
                "builder nextest run -p sim -E 'test(/telling::/)' 2>&1 | grep -E \"error|left|right|Summary\""
            ),
            vec!["builder nextest run -p sim -E 'test(/telling::/)'"]
        );
        assert!(work_of("until grep -q 'Summary\\|error' out.log; do sleep 5; done").is_empty());
        assert!(work_of("sed -n 1,19p tests/it/core/a.rs; break").is_empty());
        assert_eq!(work_of("make test > out.log 2>&1 &"), vec!["make test"]);
    }

    #[test]
    fn work_inside_a_loop_or_after_an_inline_file_is_still_work() {
        assert_eq!(
            work_of("for i in 1 2 3; do builder nextest run -p sim; done"),
            vec!["builder nextest run -p sim"]
        );
        assert_eq!(
            work_of("if ! builder check -p sim; then echo no; fi"),
            vec!["builder check -p sim"]
        );
        assert_eq!(
            work_of(
                "python3 - <<'EOF'\nopen('a.rs', 'w').write('x')\nEOF\nbuilder check -p sim 2>&1 | tail -5"
            ),
            vec!["builder check -p sim"]
        );
        assert_eq!(
            work_of("cat > a.rs <<EOF\nfn main() { builder(); }\nEOF\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_project_wrapper_from_the_definition_is_taken_off_the_work() {
        let wrappers = vec!["scripts/build-slot".to_string()];
        assert_eq!(
            work_through(
                "scripts/build-slot make check -p x 2>&1 | tail -5",
                &wrappers
            ),
            vec!["make check -p x"]
        );
        assert_eq!(
            work_through("/repo/scripts/build-slot make check -p x", &wrappers),
            vec!["make check -p x"]
        );
        assert_eq!(
            work_through("scripts/build-slot make check -p x", &[]),
            vec!["scripts/build-slot make check -p x"]
        );
    }

    #[test]
    fn a_command_that_only_writes_a_file_with_a_heredoc_is_not_work() {
        for command in [
            "gh issue comment 5 --body-file - <<'EOF'\nthe builder check passed\nEOF",
            "scripts/render --out a.md <<EOF\nmake test\nEOF\n",
            "tee notes.md <<'EOF'\nrun make test\nEOF",
        ] {
            assert!(work_of(command).is_empty(), "{command}");
        }
    }
}
