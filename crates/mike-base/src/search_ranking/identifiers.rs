const MAX_IDENTIFIERS: usize = 5;
const MIN_DIGIT_RUN: usize = 6;
const MIN_COMMIT_LEN: usize = 7;
const MAX_COMMIT_LEN: usize = 40;
const MIN_CODE_NAME_LEN: usize = 4;
const UUID_LEN: usize = 36;

pub fn query_identifiers(query: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for token in query.split_whitespace().map(clean) {
        if found.len() == MAX_IDENTIFIERS {
            break;
        }
        let seen = found.iter().any(|kept| kept.eq_ignore_ascii_case(token));
        if !seen && is_identifier(token) {
            found.push(token.to_owned());
        }
    }
    found
}

fn is_wrapper(c: char) -> bool {
    matches!(
        c,
        '"' | '\''
            | '`'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '<'
            | '>'
            | ','
            | ';'
            | ':'
            | '?'
            | '*'
            | '\u{201c}'
            | '\u{201d}'
            | '\u{2018}'
            | '\u{2019}'
    )
}

fn clean(token: &str) -> &str {
    token
        .trim_end_matches(|c: char| is_wrapper(c) || matches!(c, '.' | '!' | '/'))
        .trim_start_matches(|c: char| is_wrapper(c) || matches!(c, '.' | '/'))
}

fn is_identifier(token: &str) -> bool {
    is_number_reference(token)
        || is_ticket(token)
        || is_uuid(token)
        || is_commit(token)
        || is_digit_run(token)
        || is_code_name(token)
}

fn is_number_reference(token: &str) -> bool {
    token
        .strip_prefix(['#', '!'])
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn is_ticket(token: &str) -> bool {
    let Some((project, number)) = token.split_once('-') else {
        return false;
    };
    let mut letters = project.chars();
    letters.next().is_some_and(|c| c.is_ascii_alphabetic())
        && letters.all(|c| c.is_ascii_alphanumeric())
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
}

fn is_uuid(token: &str) -> bool {
    token.len() == UUID_LEN
        && token.char_indices().all(|(index, c)| match index {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn is_commit(token: &str) -> bool {
    (MIN_COMMIT_LEN..=MAX_COMMIT_LEN).contains(&token.len())
        && token.bytes().all(|b| b.is_ascii_hexdigit())
        && token.bytes().any(|b| b.is_ascii_digit())
        && token.bytes().any(|b| b.is_ascii_alphabetic())
}

fn is_digit_run(token: &str) -> bool {
    token.len() >= MIN_DIGIT_RUN && token.bytes().all(|b| b.is_ascii_digit())
}

fn is_code_name(token: &str) -> bool {
    if token.len() < MIN_CODE_NAME_LEN {
        return false;
    }
    let normalized = token.replace("::", "_");
    let parts: Vec<&str> = normalized.split(['_', '.', '/']).collect();
    parts.len() > 1
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()))
}
