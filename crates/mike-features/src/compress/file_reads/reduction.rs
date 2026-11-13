use super::detect::REDUCTION_MARKER_TAG;

pub struct ReductionInput<'a> {
    pub content: &'a str,
    pub file_path: &'a str,
    pub producer: &'a str,
    pub head_lines: usize,
    pub tail_lines: usize,
    pub first_line: u64,
}

pub struct Reduction {
    pub text: String,
    pub first_line: u64,
    pub head_last: u64,
    pub tail_first: u64,
    pub last_line: u64,
    pub omitted_count: usize,
}

fn cat_n_number(line: &str) -> Option<u64> {
    let (number, _) = line.split_once('\t')?;
    number.trim().parse().ok()
}

fn file_line(lines: &[&str], index: usize, first_line: u64) -> u64 {
    lines
        .get(index)
        .and_then(|line| cat_n_number(line))
        .unwrap_or(first_line + index as u64)
}

pub fn build_reduction(input: &ReductionInput<'_>) -> Reduction {
    let lines: Vec<&str> = input.content.split('\n').collect();
    let result_lines = lines.len();
    let head_end = input.head_lines.min(result_lines);
    let tail_start = result_lines.saturating_sub(input.tail_lines).max(head_end);
    let omitted_count = tail_start - head_end;
    let at = |index: usize| file_line(&lines, index, input.first_line);

    let first_line = at(0);
    let omitted_start = at(head_end);
    let omitted_end = if omitted_count == 0 {
        omitted_start.saturating_sub(1)
    } else {
        at(tail_start - 1)
    };
    let head_last = if head_end == 0 {
        first_line.saturating_sub(1)
    } else {
        at(head_end - 1)
    };
    let tail_first = at(tail_start);
    let last_line = at(result_lines - 1);

    let head_part = lines[..head_end].join("\n");
    let tail_part = lines[tail_start..].join("\n");
    let file_path = input.file_path;
    let producer = input.producer;

    let marker = format!(
        "{REDUCTION_MARKER_TAG} file={file_path} result_lines={result_lines} \
         retained={first_line}-{head_last},{tail_first}-{last_line} \
         omitted={omitted_start}-{omitted_end} ({omitted_count} lines). \
         To read this region from the current filesystem version, call {producer} with file_path=\"{file_path}\", \
         offset={omitted_start}, limit={omitted_count}. The file may have changed; the original captured result is retained by Mike."
    );

    let text = match (head_part.is_empty(), tail_part.is_empty()) {
        (true, true) => marker.clone(),
        (true, false) => format!("{marker}\n{tail_part}"),
        (false, true) => format!("{head_part}\n{marker}"),
        (false, false) => format!("{head_part}\n{marker}\n{tail_part}"),
    };

    Reduction {
        text,
        first_line,
        head_last,
        tail_first,
        last_line,
        omitted_count,
    }
}
