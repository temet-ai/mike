use super::SectionReport;
use super::cut::{char_count, cut, cut_front, head_chars};

const SEPARATOR: &str = "\n\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionBody {
    List {
        items: Vec<String>,
        noun: &'static str,
    },
    NewestFirst {
        items: Vec<String>,
        noun: &'static str,
    },
    Tail {
        text: String,
    },
    Fixed {
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocSection {
    pub id: &'static str,
    pub heading: String,
    pub body: SectionBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocated {
    pub text: String,
    pub reports: Vec<SectionReport>,
}

fn body_text(body: &SectionBody) -> String {
    match body {
        SectionBody::List { items, .. } | SectionBody::NewestFirst { items, .. }
            if items.is_empty() =>
        {
            "(none)".to_owned()
        }
        SectionBody::List { items, .. } | SectionBody::NewestFirst { items, .. } => {
            items.join(SEPARATOR)
        }
        SectionBody::Tail { text } | SectionBody::Fixed { text } if text.is_empty() => {
            "(none)".to_owned()
        }
        SectionBody::Tail { text } | SectionBody::Fixed { text } => text.clone(),
    }
}

fn whole(section: &AllocSection) -> String {
    format!("{}\n{}", section.heading, body_text(&section.body))
}

fn item_count(body: &SectionBody) -> usize {
    match body {
        SectionBody::List { items, .. } | SectionBody::NewestFirst { items, .. } => items.len(),
        SectionBody::Tail { text } | SectionBody::Fixed { text } => usize::from(!text.is_empty()),
    }
}

fn shrink_list(heading: &str, items: &[String], noun: &str, room: usize) -> (String, usize, usize) {
    let head = format!("{heading}\n");
    let budget = room.saturating_sub(char_count(&head));
    let mut kept: Vec<&String> = Vec::new();
    let mut used = 0;
    for item in items.iter().rev() {
        let omitted = items.len() - kept.len() - 1;
        let marker = if omitted > 0 {
            char_count(&format!("({omitted} earlier {noun} not shown)")) + SEPARATOR.len()
        } else {
            0
        };
        let add = char_count(item) + if kept.is_empty() { 0 } else { SEPARATOR.len() };
        if used + add + marker > budget {
            break;
        }
        used += add;
        kept.push(item);
    }
    kept.reverse();
    let omitted = items.len() - kept.len();
    let mut lines: Vec<String> = Vec::new();
    if omitted > 0 {
        lines.push(format!("({omitted} earlier {noun} not shown)"));
    }
    let mut compressed = 0;
    if kept.is_empty() {
        if let Some(newest) = items.last() {
            let marker_len = char_count(lines.first().map_or("", String::as_str)) + SEPARATOR.len();
            let space = budget.saturating_sub(marker_len).saturating_sub(40);
            lines.push(cut(newest, space));
            compressed = 1;
        }
    } else {
        lines.extend(kept.iter().map(|s| (*s).clone()));
    }
    let full = kept.len();
    (format!("{head}{}", lines.join(SEPARATOR)), full, compressed)
}

fn older_marker(omitted: usize, noun: &str) -> String {
    format!("({omitted} older {noun} not shown)")
}

fn shrink_newest_first(
    heading: &str,
    items: &[String],
    noun: &str,
    room: usize,
) -> (String, usize, usize) {
    let head = format!("{heading}\n");
    let budget = room.saturating_sub(char_count(&head));
    let mut lines: Vec<String> = Vec::new();
    let mut used = 0;
    for item in items {
        let after = items.len() - lines.len() - 1;
        let marker = if after > 0 {
            char_count(&older_marker(after, noun)) + SEPARATOR.len()
        } else {
            0
        };
        let add = char_count(item) + if lines.is_empty() { 0 } else { SEPARATOR.len() };
        if used + add + marker > budget {
            break;
        }
        used += add;
        lines.push(item.clone());
    }
    let full = lines.len();
    let mut compressed = 0;
    if let (true, Some(newest)) = (lines.is_empty(), items.first()) {
        let marker = char_count(&older_marker(items.len(), noun)) + SEPARATOR.len();
        lines.push(cut(
            newest,
            budget.saturating_sub(marker).saturating_sub(40),
        ));
        compressed = 1;
    }
    let omitted = items.len() - full - compressed;
    if omitted > 0 {
        lines.push(older_marker(omitted, noun));
    }
    (format!("{head}{}", lines.join(SEPARATOR)), full, compressed)
}

fn shrink(section: &AllocSection, room: usize) -> (String, usize, usize) {
    let (text, full, compressed) = match &section.body {
        SectionBody::List { items, noun } if !items.is_empty() => {
            shrink_list(&section.heading, items, noun, room)
        }
        SectionBody::NewestFirst { items, noun } if !items.is_empty() => {
            shrink_newest_first(&section.heading, items, noun, room)
        }
        SectionBody::Tail { text } if !text.is_empty() => {
            let space = room.saturating_sub(char_count(&section.heading) + 1 + 40);
            (
                format!("{}\n{}", section.heading, cut_front(text, space)),
                0,
                1,
            )
        }
        SectionBody::Fixed { text } if !text.is_empty() => {
            let space = room.saturating_sub(char_count(&section.heading) + 1 + 32);
            (format!("{}\n{}", section.heading, cut(text, space)), 0, 1)
        }
        _ => (whole(section), 0, 0),
    };
    if char_count(&text) > room {
        (head_chars(&text, room).to_owned(), full, compressed.max(1))
    } else {
        (text, full, compressed)
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn shares(needs: &[usize], fixed: &[bool], n: usize, seps: usize) -> Vec<usize> {
    let k = needs.len().max(1);
    let reserved: usize = needs
        .iter()
        .zip(fixed)
        .filter(|(_, f)| **f)
        .map(|(n, _)| *n)
        .sum();
    let open = n.saturating_sub(seps).saturating_sub(reserved);
    let flexible = fixed.iter().filter(|f| !**f).count().max(1);
    let floor = open / (2 * flexible.min(k));
    let mut alloc: Vec<usize> = needs
        .iter()
        .zip(fixed)
        .map(|(need, f)| if *f { *need } else { (*need).min(floor) })
        .collect();
    let mut remaining = open.saturating_sub(
        alloc
            .iter()
            .zip(fixed)
            .filter(|(_, f)| !**f)
            .map(|(a, _)| *a)
            .sum(),
    );
    for (i, need) in needs.iter().enumerate() {
        if fixed[i] {
            continue;
        }
        let extra = need.saturating_sub(alloc[i]).min(remaining);
        alloc[i] += extra;
        remaining -= extra;
    }
    alloc
}

pub fn allocate(sections: &[AllocSection], n: usize, reserved: &[&'static str]) -> Allocated {
    let wholes: Vec<String> = sections.iter().map(whole).collect();
    let seps = SEPARATOR.len() * sections.len().saturating_sub(1);
    let total: usize = wholes.iter().map(|w| char_count(w)).sum::<usize>() + seps;
    if total <= n {
        let reports = sections
            .iter()
            .zip(&wholes)
            .map(|(s, w)| SectionReport::whole(s.id, item_count(&s.body), char_count(w)))
            .collect();
        return Allocated {
            text: wholes.join(SEPARATOR),
            reports,
        };
    }
    let needs: Vec<usize> = wholes.iter().map(|w| char_count(w)).collect();
    let fixed: Vec<bool> = sections.iter().map(|s| reserved.contains(&s.id)).collect();
    let alloc = shares(&needs, &fixed, n, seps);
    let mut parts = Vec::with_capacity(sections.len());
    let mut reports = Vec::with_capacity(sections.len());
    for ((section, whole_text), room) in sections.iter().zip(&wholes).zip(alloc) {
        let (text, full, compressed) = if char_count(whole_text) <= room {
            (whole_text.clone(), item_count(&section.body), 0)
        } else {
            shrink(section, room)
        };
        reports.push(SectionReport {
            id: section.id.to_owned(),
            items_total: to_u32(item_count(&section.body)),
            items_full: to_u32(full),
            items_compressed: to_u32(compressed),
            items_in_spans: 0,
            chars: to_u32(char_count(&text)),
        });
        parts.push(text);
    }
    let text = parts.join(SEPARATOR);
    let text = if char_count(&text) > n {
        head_chars(&text, n).to_owned()
    } else {
        text
    };
    Allocated { text, reports }
}
