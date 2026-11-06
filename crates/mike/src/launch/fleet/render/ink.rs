const RESET: &str = "\x1b[0m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rgb(pub(super) u8, pub(super) u8, pub(super) u8);

pub(super) const INK: Rgb = Rgb(0xc0, 0xca, 0xf5);
pub(super) const DIM: Rgb = Rgb(0x8a, 0x93, 0xbd);
pub(super) const FAINT: Rgb = Rgb(0x56, 0x5f, 0x89);
pub(super) const GREEN: Rgb = Rgb(0x9e, 0xce, 0x6a);
pub(super) const YELLOW: Rgb = Rgb(0xe0, 0xaf, 0x68);
pub(super) const RED: Rgb = Rgb(0xf7, 0x76, 0x8e);
pub(super) const BLUE: Rgb = Rgb(0x7a, 0xa2, 0xf7);
pub(super) const ORANGE: Rgb = Rgb(0xff, 0x9e, 0x64);
pub(super) const PURPLE: Rgb = Rgb(0xbb, 0x9a, 0xf7);
const BADGE_TEXT: Rgb = Rgb(0x1a, 0x1b, 0x26);

const ROLE_COLOURS: [(&str, Rgb); 10] = [
    ("coordinator", Rgb(0xf5, 0x8a, 0xc8)),
    ("observer", Rgb(0xf7, 0x76, 0x8e)),
    ("acceptance", Rgb(0xe0, 0xaf, 0x68)),
    ("rust-developer", Rgb(0xff, 0x9e, 0x64)),
    ("frontend-developer", Rgb(0x9e, 0xce, 0x6a)),
    ("db-dev", Rgb(0x7a, 0xa2, 0xf7)),
    ("ui-designer", Rgb(0xbb, 0x9a, 0xf7)),
    ("motion-designer", Rgb(0x7d, 0xcf, 0xff)),
    ("unit-tester", Rgb(0x2a, 0xc3, 0xde)),
    ("unit", Rgb(0xb9, 0xf2, 0x7c)),
];

pub(super) fn role_colour(role: &str) -> Option<Rgb> {
    ROLE_COLOURS
        .iter()
        .find(|(name, _)| *name == role)
        .map(|(_, colour)| *colour)
}

pub(super) fn fleet_state_colour(state: &str) -> Rgb {
    match state {
        "running" | "done" => GREEN,
        "failed" => RED,
        _ => YELLOW,
    }
}

pub(super) fn paint(text: &str, colour: Rgb, bold: bool) -> String {
    if text.is_empty() {
        return String::new();
    }
    let weight = if bold { "1;" } else { "" };
    let Rgb(r, g, b) = colour;
    format!("\x1b[{weight}38;2;{r};{g};{b}m{text}{RESET}")
}

pub(super) fn badge(text: &str, background: Rgb) -> String {
    let Rgb(br, bg, bb) = background;
    let Rgb(r, g, b) = BADGE_TEXT;
    format!("\x1b[1;48;2;{br};{bg};{bb};38;2;{r};{g};{b}m{text}{RESET}")
}

pub(crate) fn strip(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for skipped in chars.by_ref() {
                if skipped == 'm' {
                    break;
                }
            }
        } else {
            plain.push(c);
        }
    }
    plain
}

pub(super) fn visible_len(text: &str) -> usize {
    strip(text).chars().count()
}

pub(super) fn fit(row: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if visible_len(row) <= width {
        return row.to_string();
    }
    let keep = width - 1;
    let mut out = String::with_capacity(row.len());
    let mut seen = 0;
    let mut chars = row.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            out.push(c);
            for escaped in chars.by_ref() {
                out.push(escaped);
                if escaped == 'm' {
                    break;
                }
            }
            continue;
        }
        if seen == keep {
            break;
        }
        out.push(c);
        seen += 1;
    }
    out.push_str(RESET);
    out.push('…');
    out
}

pub(super) fn pad(text: &str, width: usize) -> String {
    let shown = visible_len(text);
    if shown <= width {
        format!("{text}{}", " ".repeat(width - shown))
    } else {
        format!("{} ", fit(text, width.saturating_sub(1)))
    }
}
