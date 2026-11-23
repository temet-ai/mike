#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Located {
    pub start: usize,
    pub end: usize,
}

struct Normalised {
    text: String,
    starts: Vec<usize>,
    ends: Vec<usize>,
}

fn normalise(source: &str) -> Normalised {
    let mut out = Normalised {
        text: String::with_capacity(source.len()),
        starts: Vec::with_capacity(source.len()),
        ends: Vec::with_capacity(source.len()),
    };
    let mut pending_space: Option<usize> = None;
    for (index, ch) in source.char_indices() {
        let end = index + ch.len_utf8();
        if ch.is_whitespace() {
            pending_space.get_or_insert(index);
            continue;
        }
        if let Some(space_start) = pending_space.take()
            && !out.text.is_empty()
        {
            out.text.push(' ');
            out.starts.push(space_start);
            out.ends.push(index);
        }
        let mut buffer = [0_u8; 4];
        out.text.push_str(ch.encode_utf8(&mut buffer));
        for _ in 0..ch.len_utf8() {
            out.starts.push(index);
            out.ends.push(end);
        }
    }
    out
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn locate(body: &str, passage: &str) -> Option<Located> {
    let wanted = collapse(passage);
    if wanted.is_empty() {
        return None;
    }
    let normalised = normalise(body);
    let at = normalised.text.find(&wanted)?;
    let last = at + wanted.len() - 1;
    Some(Located {
        start: *normalised.starts.get(at)?,
        end: *normalised.ends.get(last)?,
    })
}

pub fn covered_share(found: usize, returned: usize) -> f64 {
    if returned == 0 {
        1.0
    } else {
        found as f64 / returned as f64
    }
}

pub const SECTION_CHARS: usize = 40_000;

fn is_heading(line: &str) -> bool {
    line.starts_with('#')
}

pub fn sections(body: &str, max_chars: usize) -> Vec<(usize, &str)> {
    if body.chars().count() <= max_chars {
        return vec![(0, body)];
    }
    let mut cuts = vec![0_usize];
    let mut section_start = 0_usize;
    let mut chars_in = 0_usize;
    let mut last_heading: Option<usize> = None;
    let mut last_line: Option<usize> = None;
    let mut offset = 0_usize;
    for line in body.split_inclusive('\n') {
        let len = line.chars().count();
        if chars_in + len > max_chars && offset > section_start {
            let cut = last_heading
                .filter(|h| *h > section_start)
                .or(last_line.filter(|l| *l > section_start))
                .unwrap_or(offset);
            cuts.push(cut);
            section_start = cut;
            chars_in = body[cut..offset].chars().count();
            last_heading = None;
            last_line = None;
        }
        if is_heading(line) && offset > section_start {
            last_heading = Some(offset);
        }
        if line.trim().is_empty() && offset > section_start {
            last_line = Some(offset + line.len());
        }
        chars_in += len;
        offset += line.len();
    }
    cuts.push(body.len());
    cuts.windows(2).map(|w| (w[0], &body[w[0]..w[1]])).collect()
}
