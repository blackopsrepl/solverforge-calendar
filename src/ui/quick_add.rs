/* Quick-add event bar — replaces the status bar for fast event entry.  */

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::theme::theme;

pub fn render_quick_add(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();

    let cursor = if app.cursor_visible() {
        "\u{2588}"
    } else {
        " "
    }; // █

    let spans: Vec<Span> = vec![
        Span::styled(" / ", t.quick_add_label()),
        Span::styled("  ", t.status_bar()),
        Span::styled(
            format!("{}{}", app.quick_add_input, cursor),
            t.search_input(),
        ),
        Span::styled("  ", t.status_bar()),
        Span::styled("Enter: create  Esc: cancel", t.dimmed()),
    ];

    let line = Line::from(spans);
    let para = Paragraph::new(line).style(t.status_bar());
    frame.render_widget(para, area);
}
