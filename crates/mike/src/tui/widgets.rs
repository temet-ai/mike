use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block as RBlock, Borders, Clear, List as RList, ListItem, ListState, Paragraph, Wrap,
};

use super::theme::{self, Tone};

pub const NAV_ITEMS: [&str; 4] = ["Services", "Config", "Setup", "Sessions"];

pub const NAV_WIDTH: u16 = 18;

pub const NAV_HEIGHT: u16 = NAV_ITEMS.len() as u16 + 2;

pub fn render_nav_rail(frame: &mut Frame, area: Rect, selected: &mut ListState, focused: bool) {
    let items: Vec<ListItem> = NAV_ITEMS
        .iter()
        .map(|label| ListItem::new(*label).style(Style::default().fg(theme::FG_1)))
        .collect();
    let highlight_symbol = format!("{} ", theme::glyph::SELECTION);
    let list = RList::new(items)
        .block(block("menu", Tone::Rust, focused))
        .highlight_symbol(highlight_symbol.as_str())
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme::RUST)
                .bg(theme::BG_3),
        );
    frame.render_stateful_widget(list, area, selected);
}

pub fn render_top_bar(frame: &mut Frame, area: Rect, running: bool) {
    let (status_glyph, status_color, status_word) = if running {
        (theme::glyph::FILLED_CIRCLE, theme::ACID, "online")
    } else {
        (theme::glyph::OPEN_CIRCLE, theme::RED, "stopped")
    };
    let left = " mike";
    let right = format!(
        "{status_glyph} {status_word} {} configured ",
        theme::glyph::SEP
    );
    let pad = (area.width as usize)
        .saturating_sub(left.chars().count() + right.chars().count())
        .max(1);
    let line = Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .fg(theme::FG_1)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(pad)),
        Span::styled(right, Style::default().fg(status_color)),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme::BG_1)),
        area,
    );
}

pub fn block(title: &str, tone: Tone, focused: bool) -> RBlock<'static> {
    let color = tone.color();
    let border_style = if focused {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::FG_3)
    };
    RBlock::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {} ", title.to_uppercase()),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
}

pub fn block_with_hotkey(title: &str, hotkey: char, tone: Tone, focused: bool) -> RBlock<'static> {
    let color = tone.color();
    let border_style = if focused {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::FG_3)
    };
    let upper = title.to_uppercase();
    let title_style = Style::default().fg(color).add_modifier(Modifier::BOLD);
    let mut spans = vec![Span::styled(" ".to_string(), title_style)];
    match upper
        .char_indices()
        .find(|(_, c)| c.eq_ignore_ascii_case(&hotkey))
    {
        Some((idx, marked)) => {
            let before = &upper[..idx];
            let after = &upper[idx + marked.len_utf8()..];
            spans.push(Span::styled(before.to_string(), title_style));
            spans.push(Span::styled(
                marked.to_string(),
                title_style.add_modifier(Modifier::UNDERLINED),
            ));
            spans.push(Span::styled(after.to_string(), title_style));
        }
        None => {
            spans.push(Span::styled(upper, title_style));
            spans.push(Span::styled(
                format!(" [⌃{}]", hotkey.to_ascii_lowercase()),
                Style::default().fg(theme::FG_3),
            ));
        }
    }
    spans.push(Span::styled(" ".to_string(), title_style));
    RBlock::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Line::from(spans))
}

fn key_hint_text(key: &str, label: &str) -> String {
    format!("[{key}] {label}")
}

pub fn render_status_bar(
    frame: &mut Frame,
    area: Rect,
    mode: &str,
    mode_color: Color,
    message: &str,
    keys: &[(&str, &str)],
) {
    let badge = format!(" {mode:<9}");
    let mut right_text = keys
        .iter()
        .map(|(k, l)| key_hint_text(k, l))
        .collect::<Vec<_>>()
        .join("  ");
    if !right_text.is_empty() {
        right_text.push_str("  ");
    }
    right_text.push_str("[⌃ /] shortcuts");

    let left_width = badge.chars().count() + 2 + message.chars().count();
    let total = area.width as usize;
    let right_width = right_text.chars().count();
    let pad = total.saturating_sub(left_width + right_width).max(1);

    let line = Line::from(vec![
        Span::styled(
            badge,
            Style::default()
                .fg(theme::BG_0)
                .bg(mode_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(message.to_string(), Style::default().fg(theme::FG_2)),
        Span::raw(" ".repeat(pad)),
        Span::styled(right_text, Style::default().fg(theme::FG_3)),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme::BG_1)),
        area,
    );
}

pub struct ShortcutsSection {
    pub heading: &'static str,
    pub entries: &'static [(&'static str, &'static str)],
}

pub const NAV_RAIL_SHORTCUTS: ShortcutsSection = ShortcutsSection {
    heading: "nav rail",
    entries: &[
        ("↑↓/jk", "browse Services/Config/Setup"),
        ("⇥", "focus this panel"),
        ("⇧⇥", "back to the rail"),
    ],
};

pub fn render_shortcuts_modal(frame: &mut Frame, area: Rect, sections: &[ShortcutsSection]) {
    let popup = centered_rect(60, 60, area);
    frame.render_widget(Clear, popup);

    let mut lines = Vec::new();
    for section in sections {
        lines.push(Line::from(Span::styled(
            section.heading,
            Style::default()
                .fg(theme::RUST)
                .add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in section.entries {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {key:<10}"),
                    Style::default().fg(theme::UV).add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc.to_string(), Style::default().fg(theme::FG_1)),
            ]));
        }
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(
        "esc / ? / ⌃ / close",
        Style::default().fg(theme::FG_DIM),
    )));

    let block = block("shortcuts", Tone::Rust, true);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().bg(theme::BG_2))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate_len = if current.is_empty() {
                word.chars().count()
            } else {
                current.chars().count() + 1 + word.chars().count()
            };
            if candidate_len > width && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
            while current.chars().count() > width {
                let split_at = current
                    .char_indices()
                    .nth(width)
                    .map(|(i, _)| i)
                    .unwrap_or(current.len());
                let rest = current.split_off(split_at);
                lines.push(std::mem::take(&mut current));
                current = rest;
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_breaks_on_whitespace_under_width() {
        let wrapped = wrap_text("the quick brown fox jumps", 10);
        assert_eq!(wrapped, vec!["the quick", "brown fox", "jumps"]);
    }

    #[test]
    fn wrap_text_hard_splits_a_single_overlong_word() {
        let wrapped = wrap_text("supercalifragilistic", 8);
        assert_eq!(wrapped, vec!["supercal", "ifragili", "stic"]);
        assert!(
            wrapped.iter().all(|l| l.chars().count() <= 8),
            "no line may exceed the requested width: {wrapped:?}"
        );
    }

    #[test]
    fn wrap_text_never_drops_content() {
        let input = "postgres://user:pass@host:5432/db is a long connection string";
        let wrapped = wrap_text(input, 40);
        let rejoined = wrapped.join(" ");
        assert_eq!(
            rejoined.split_whitespace().collect::<Vec<_>>(),
            input.split_whitespace().collect::<Vec<_>>(),
            "wrapping must never lose or reorder words: {wrapped:?}"
        );
    }

    #[test]
    fn wrap_text_hard_split_preserves_every_character() {
        let input = "postgres://user:pass@host:5432/db";
        let wrapped = wrap_text(input, 12);
        assert_eq!(
            wrapped.concat(),
            input,
            "a hard-split overlong token must reassemble byte-for-byte: {wrapped:?}"
        );
    }
}
