use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::Wait;
use crate::finding::Finding;
use crate::tui::client::FirstRequest;
use crate::tui::screens::setup::panel::finding_lines;
use crate::tui::theme::{self, Tone};
use crate::tui::widgets::{block, wrap_text};

fn heading(text: &str) -> Line<'static> {
    Line::styled(
        text.to_string(),
        Style::default().fg(theme::ICE).add_modifier(Modifier::BOLD),
    )
}

fn plain(text: &str, width: usize, color: ratatui::style::Color) -> Vec<Line<'static>> {
    wrap_text(text, width.max(10))
        .into_iter()
        .map(|line| Line::styled(format!("  {line}"), Style::default().fg(color)))
        .collect()
}

pub(super) fn describe(request: &FirstRequest) -> String {
    let model = request.model.as_deref().unwrap_or("no model named");
    let project = request
        .project
        .as_deref()
        .map(|p| format!(", project {p}"))
        .unwrap_or_default();
    format!(
        "Request {} arrived at {}: {} {}, {model}, {}, session {}{project}.",
        request.id,
        request.created_at,
        request.method,
        request.path,
        request.status,
        request.session_id
    )
}

fn waiting_lines(wait: &Wait, width: usize) -> Vec<Line<'static>> {
    match wait {
        Wait::Waiting => plain(
            "Waiting for the first request from Claude Code. Press c to stop waiting.",
            width,
            theme::FG_2,
        ),
        Wait::Stopped => plain(
            "Stopped waiting. Press r to check again and wait again.",
            width,
            theme::FG_3,
        ),
        Wait::Arrived(request) => {
            let mut lines = plain(
                "The first captured request is in the proxy:",
                width,
                theme::ACID,
            );
            lines.extend(plain(&describe(request), width, theme::FG_1));
            lines
        }
    }
}

pub(super) fn lines(
    launch: &str,
    wait: &Wait,
    findings: &[Finding],
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![heading("Start Claude Code")];
    lines.extend(plain(launch, width, theme::FG_1));
    lines.push(Line::raw(""));
    lines.push(heading("First request"));
    lines.extend(waiting_lines(wait, width));
    lines.push(Line::raw(""));
    lines.push(heading("What was checked"));
    for finding in findings {
        lines.extend(finding_lines(finding, width));
    }
    lines
}

pub(super) fn draw(frame: &mut Frame, area: Rect, launch: &str, wait: &Wait, findings: &[Finding]) {
    let b = block("ready", Tone::Acid, true);
    let inner = b.inner(area);
    frame.render_widget(b, area);
    let lines = lines(launch, wait, findings, inner.width as usize);
    frame.render_widget(Paragraph::new(lines), inner);
}
