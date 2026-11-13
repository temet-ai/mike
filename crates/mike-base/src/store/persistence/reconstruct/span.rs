pub fn top_level_array_span(body: &[u8], key: &str) -> Option<(usize, usize)> {
    let mut needle = Vec::with_capacity(key.len() + 3);
    needle.push(b'"');
    needle.extend_from_slice(key.as_bytes());
    needle.extend_from_slice(b"\":");

    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0usize;
    while i < body.len() {
        let c = body[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                if depth == 1 && body[i..].starts_with(&needle) {
                    let mut open = i + needle.len();
                    while matches!(body.get(open), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                        open += 1;
                    }
                    if body.get(open) != Some(&b'[') {
                        return None;
                    }
                    let start = open + 1;
                    return array_close(body, open).map(|close| (start, close));
                }
                in_string = true;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

fn array_close(body: &[u8], open: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut i = open;
    while i < body.len() {
        let c = body[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'[' | b'{' => depth += 1,
            b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}
