const ELLIPSIS: char = '…';
const MIN_BODY_BYTES: usize = 40;
const MIN_CLIP_BYTES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub body: String,
    pub refs: Vec<String>,
}

impl Item {
    pub fn new(body: String, refs: Vec<String>) -> Self {
        Self { body, refs }
    }

    fn line(&self) -> String {
        if self.refs.is_empty() {
            return self.body.clone();
        }
        format!("{} [{}]", self.body, self.refs.join(" "))
    }
}

pub fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn ellipsize(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }
    let keep = max_bytes.saturating_sub(ELLIPSIS.len_utf8());
    let cut = (0..=keep)
        .rev()
        .find(|index| text.is_char_boundary(*index))
        .unwrap_or(0);
    format!("{}{ELLIPSIS}", text[..cut].trim_end())
}

pub fn render(heading: Option<&str>, items: &[Item]) -> String {
    let lines: Vec<String> = items.iter().map(Item::line).collect();
    match (heading, lines.as_slice()) {
        (None, _) => lines.join("\n"),
        (Some(heading), [only]) => format!("{heading}: {only}"),
        (Some(heading), _) => {
            let bullets: Vec<String> = lines.iter().map(|line| format!("- {line}")).collect();
            format!("{heading}:\n{}", bullets.join("\n"))
        }
    }
}

fn shrink_longest(items: &mut [Item], excess: usize) -> bool {
    let Some(item) = items
        .iter_mut()
        .filter(|item| item.body.len() > MIN_BODY_BYTES)
        .max_by_key(|item| item.body.len())
    else {
        return false;
    };
    let target = item.body.len().saturating_sub(excess).max(MIN_BODY_BYTES);
    item.body = ellipsize(&item.body, target);
    true
}

fn clip_single(heading: Option<&str>, mut items: Vec<Item>, budget: usize) -> Option<Vec<Item>> {
    let rendered = render(heading, &items).len() + 1;
    let item = items.first_mut()?;
    let overhead = rendered.saturating_sub(item.body.len());
    let room = budget.checked_sub(overhead)?;
    if room < MIN_CLIP_BYTES {
        return None;
    }
    item.body = ellipsize(&item.body, room);
    Some(items)
}

pub fn fit(heading: Option<&str>, mut items: Vec<Item>, budget: usize) -> Option<Vec<Item>> {
    loop {
        if items.is_empty() {
            return None;
        }
        let needed = render(heading, &items).len() + 1;
        if needed <= budget {
            return Some(items);
        }
        if shrink_longest(&mut items, needed - budget) {
            continue;
        }
        if items.len() > 1 {
            items.pop();
            continue;
        }
        return clip_single(heading, items, budget);
    }
}
