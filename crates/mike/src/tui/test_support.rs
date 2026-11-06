use ratatui::Terminal;
use ratatui::backend::TestBackend;

pub(crate) fn render_to_lines<F: FnOnce(&mut ratatui::Frame, ratatui::layout::Rect)>(
    w: u16,
    h: u16,
    draw_fn: F,
) -> Vec<String> {
    let backend = TestBackend::new(w, h);
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => panic!("TestBackend terminal must construct: {e}"),
    };
    if let Err(e) = terminal.draw(|frame| draw_fn(frame, frame.area())) {
        panic!("draw must not error: {e}");
    }
    let buffer = terminal.backend().buffer();
    (0..h)
        .map(|y| (0..w).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect()
}

pub(crate) fn render_to_string<F: FnOnce(&mut ratatui::Frame, ratatui::layout::Rect)>(
    w: u16,
    h: u16,
    draw_fn: F,
) -> String {
    render_to_lines(w, h, draw_fn).join("\n")
}
