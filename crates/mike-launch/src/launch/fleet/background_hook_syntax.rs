#[derive(Default)]
struct Scan {
    segments: Vec<String>,
    current: String,
    quote: Option<char>,
}

impl Scan {
    fn cut(&mut self) {
        self.segments.push(std::mem::take(&mut self.current));
    }

    fn quoted(&mut self, c: char, rest: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
        self.current.push(c);
        if self.quote == Some(c) {
            self.quote = None;
        } else if self.quote == Some('"') {
            if matches!(c, '$' | '`') {
                return false;
            }
            if c == '\\' {
                self.current.extend(rest.next());
            }
        }
        true
    }

    fn bare(&mut self, c: char, rest: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
        match c {
            '\'' | '"' => {
                self.quote = Some(c);
                self.current.push(c);
            }
            '$' | '`' | '<' | '(' | ')' | '{' | '}' | '#' => return false,
            '\\' if rest.next_if_eq(&'\n').is_some() => {}
            '\\' => return false,
            ';' | '\n' | '|' => self.cut(),
            '&' if self.current.ends_with('>') || rest.peek() == Some(&'>') => {
                self.current.push(c);
            }
            '&' if rest.next_if_eq(&'&').is_some() => self.cut(),
            '&' => return false,
            _ => self.current.push(c),
        }
        true
    }
}

fn segments(command: &str) -> Option<Vec<String>> {
    let mut scan = Scan::default();
    let mut rest = command.chars().peekable();
    while let Some(c) = rest.next() {
        let supported = match scan.quote {
            Some(_) => scan.quoted(c, &mut rest),
            None => scan.bare(c, &mut rest),
        };
        if !supported {
            return None;
        }
    }
    if scan.quote.is_some() {
        return None;
    }
    scan.cut();
    Some(scan.segments)
}

fn assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn duration(word: &str) -> bool {
    let digits = word.strip_suffix(['s', 'm', 'h', 'd']).unwrap_or(word);
    digits
        .parse::<f64>()
        .is_ok_and(|n| n.is_finite() && n > 0.0)
}

fn skip_environment(words: &mut std::iter::Peekable<std::str::SplitWhitespace<'_>>) {
    while words.peek().is_some_and(|word| assignment(word)) {
        words.next();
    }
    if words.peek() == Some(&"env") {
        words.next();
        while words.peek().is_some_and(|word| assignment(word)) {
            words.next();
        }
    }
}

fn names_program(word: &str, wanted: &str) -> bool {
    word == wanted || word.rsplit('/').next() == Some(wanted)
}

fn declared_by(words: &[&str], declared: &str) -> bool {
    let wanted: Vec<&str> = declared.split_whitespace().collect();
    match (words, wanted.as_slice()) {
        ([program, rest @ ..], [named, subcommand @ ..]) => {
            names_program(program, named) && rest.starts_with(subcommand)
        }
        _ => false,
    }
}

fn invocation(segment: &str, declared: &[String]) -> bool {
    let mut words = segment.split_whitespace().peekable();
    skip_environment(&mut words);
    if words.peek() == Some(&"timeout") {
        words.next();
        if !words.next().is_some_and(duration) {
            return false;
        }
    }
    if words.peek() == Some(&"command") {
        words.next();
    }
    let words: Vec<&str> = words.collect();
    declared.iter().any(|one| declared_by(&words, one))
}

pub fn declared_command(command: &str, declared: &[String]) -> bool {
    let Some(parts) = segments(command) else {
        return false;
    };
    let control = [
        "for", "while", "until", "if", "then", "do", "else", "elif", "done", "fi", "case", "esac",
        "function", "select", "eval",
    ];
    if parts.iter().any(|part| {
        part.split_whitespace()
            .next()
            .is_some_and(|word| control.contains(&word))
    }) {
        return false;
    }
    parts.iter().any(|part| invocation(part, declared))
}
