pub const MIN_SUBSTANTIVE_CHARS: usize = 80;

fn frontmatter_end(body: &str) -> usize {
    let Some(rest) = body.strip_prefix("---\n") else {
        return 0;
    };
    let Some(at) = rest.find("\n---") else {
        return 0;
    };
    let close = 4 + at + 1;
    body[close..].find('\n').map_or(body.len(), |n| close + n + 1)
}

fn floor_boundary(body: &str, mut at: usize) -> usize {
    at = at.min(body.len());
    while !body.is_char_boundary(at) {
        at -= 1;
    }
    at
}

pub fn expand_to_lines(body: &str, start: usize, end: usize) -> (usize, usize) {
    let start = floor_boundary(body, start);
    let end = floor_boundary(body, end).max(start);
    let line_start = body[..start].rfind('\n').map_or(0, |i| i + 1);
    let from = if body[line_start..start].trim().is_empty() {
        line_start
    } else {
        body[start..].find('\n').map_or(body.len(), |i| start + i + 1)
    };
    let to = if end == 0 || body[..end].ends_with('\n') {
        end
    } else {
        let line_end = body[end..].find('\n').map_or(body.len(), |i| end + i + 1);
        if body[end..line_end].trim().is_empty() {
            line_end
        } else {
            body[..end].rfind('\n').map_or(0, |i| i + 1)
        }
    };
    (from.min(to), to)
}

pub fn substantive_chars(body: &str, from: usize, to: usize) -> usize {
    let front = frontmatter_end(body);
    let mut offset = from;
    let mut count = 0;
    for line in body[from..to].split_inclusive('\n') {
        if offset >= front && !line.starts_with('#') {
            count += line.chars().filter(|c| !c.is_whitespace()).count();
        }
        offset += line.len();
    }
    count
}

pub fn worth_splitting(body: &str, uncovered: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in uncovered {
        let (from, to) = expand_to_lines(body, *start, *end);
        match merged.last_mut() {
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => merged.push((from, to)),
        }
    }
    merged
        .into_iter()
        .filter(|(from, to)| from < to && substantive_chars(body, *from, *to) > MIN_SUBSTANTIVE_CHARS)
        .collect()
}
