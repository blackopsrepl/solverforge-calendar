/* Week view — vertical time grid with: - 7-column day layout (Mon–Sun) - Event ribbons with colored left rails (▌ per calendar color) - Pulsing "Now" beam showing current time - Past-hour dimming - Inline project context in status area  */
/* Week view — vertical time grid with: - 7-column day layout (Mon–Sun) - Event ribbons with colored left rails (▌ per calendar color) - Pulsing "Now" beam showing current time - Past-hour dimming - Inline project context in status area  */
/* Week view — vertical time grid with: - 7-column day layout (Mon–Sun) - Event ribbons with colored left rails (▌ per calendar color) - Pulsing "Now" beam showing current time - Past-hour dimming - Inline project context in status area  */
/* Week view — vertical time grid with: - 7-column day layout (Mon–Sun) - Event ribbons with colored left rails (▌ per calendar color) - Pulsing "Now" beam showing current time - Past-hour dimming - Inline project context in status area  */
/* Week view — vertical time grid with: - 7-column day layout (Mon–Sun) - Event ribbons with colored left rails (▌ per calendar color) - Pulsing "Now" beam showing current time - Past-hour dimming - Inline project context in status area  */
/* Week view — vertical time grid with: - 7-column day layout (Mon–Sun) - Event ribbons with colored left rails (▌ per calendar color) - Pulsing "Now" beam showing current time - Past-hour dimming - Inline project context in status area  */

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Timelike, Weekday};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::theme::theme;
use crate::ui::util::{format_time_hhmm, truncate, WEEKDAY_SHORT};

/* Hours to show in the time grid (start hour, end hour inclusive). */
const HOUR_START: u32 = 0;
const HOUR_END: u32 = 23;

pub fn render_week(app: &App, frame: &mut Frame, area: Rect) {
    let t = theme();

    let block = Block::default()
        .title(" Week ")
        .title_style(t.accent_style().add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(t.border_focused());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Week start (Monday of focused_date's week)
    let days_from_mon = chrono::Datelike::weekday(&app.focused_date).num_days_from_monday() as i64;
    let week_start = app.focused_date - Duration::days(days_from_mon);

    let [header_area, grid_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);

    render_week_header(app, frame, header_area, week_start);
    render_time_grid(app, frame, grid_area, week_start, 7);
}

fn render_week_header(app: &App, frame: &mut Frame, area: Rect, week_start: NaiveDate) {
    let t = theme();
    let today = Local::now().date_naive();
    // Time label column (4 chars)
    let time_col_w = 6u16;
    let day_col_w = area.width.saturating_sub(time_col_w) / 7;

    let mut spans: Vec<Span> = vec![
        Span::styled(format!("{:^6}", ""), t.dimmed()), // time label column
    ];

    for d in 0..7u32 {
        let date = week_start + Duration::days(d as i64);
        let is_today = date == today;
        let is_selected = date == app.focused_date;
        let weekday_name = WEEKDAY_SHORT[d as usize];
        let label = format!("{} {}", weekday_name, date.day());
        let padded = format!("{:^width$}", label, width = day_col_w as usize);

        let style = if is_today {
            t.today_cell().add_modifier(Modifier::BOLD)
        } else if is_selected {
            t.accent_style().add_modifier(Modifier::BOLD)
        } else if d >= 5 {
            t.weekend()
        } else {
            t.header_label()
        };
        spans.push(Span::styled(padded, style));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/* Renders the time grid for `num_days` days starting from `start_date`. */
pub fn render_time_grid(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    start_date: NaiveDate,
    num_days: u32,
) {
    let t = theme();
    let today = Local::now().date_naive();
    let now = Local::now();
    let now_minutes = now.hour() * 60 + now.minute();

    let time_col_w = 6u16;
    let day_col_w = area.width.saturating_sub(time_col_w) / num_days as u16;
    let total_hours = HOUR_END - HOUR_START + 1;

    // Visible hours window based on week_scroll
    let scroll_offset = app.week_scroll.max(0) as u32;
    let visible_rows = area.height as u32;
    let visible_end_hour = (scroll_offset + visible_rows / 2 + 1).min(HOUR_END);

    // Smart time compression: identify truly empty hours
    let loaded_events = app.visible_events();

    // Build per-hour, per-day event slots
    // For each hour row, render the time label and event content

    // Row height: 2 lines per hour (for event content display)
    let row_h = 2u16;
    let max_visible_hours = (area.height / row_h) as u32;

    let mut y = area.y;
    for hour_offset in 0..max_visible_hours {
        let hour = scroll_offset + hour_offset;
        if hour > HOUR_END || y + row_h > area.y + area.height {
            break;
        }

        let is_now_hour = today >= start_date
            && today < start_date + Duration::days(num_days as i64)
            && now.hour() == hour;

        let hour_is_past = {
            let hour_minutes = hour * 60;
            today == Local::now().date_naive() && hour_minutes + 59 < now_minutes
        };

        // Time label
        let label_area = Rect {
            x: area.x,
            y,
            width: time_col_w,
            height: row_h,
        };

        let label_style = if hour_is_past {
            t.past_dim()
        } else {
            t.dimmed()
        };
        let label = format!("{:02}:00", hour);

        // Now beam: draw the beam line across this hour at the current minute position
        if is_now_hour {
            // Draw current-time beam — pulsing using tick_count
            let beam_y = y + ((now.minute() as u16 * row_h) / 60).min(row_h - 1);
            let pulse = (app.tick_count / 2) % 3; // 0, 1, 2 for brightness oscillation
            let beam_char = match pulse {
                0 => "\u{2501}", // ━ thick
                1 => "\u{2500}", // ─ normal
                _ => "\u{254c}", // ╌ dashed
            };
            let beam_style = t.now_beam().add_modifier(Modifier::BOLD);
            let beam_label = format!("{:02}:{:02}", now.hour(), now.minute());

            // Beam spans full width
            let beam_spans: Vec<Span> = vec![
                Span::styled(
                    format!("{:<5}", beam_label),
                    t.now_beam().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    beam_char.repeat((area.width.saturating_sub(6)) as usize),
                    beam_style,
                ),
            ];

            let beam_area = Rect {
                x: area.x,
                y: beam_y,
                width: area.width,
                height: 1,
            };
            frame.render_widget(Paragraph::new(Line::from(beam_spans)), beam_area);

            // Label above beam
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(label, t.now_beam()))),
                label_area,
            );
        } else {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(label, label_style))),
                label_area,
            );
        }

        // Day event columns
        for d in 0..num_days {
            let date = start_date + Duration::days(d as i64);
            let col_x = area.x + time_col_w + (d as u16 * day_col_w);

            let col_area = Rect {
                x: col_x,
                y,
                width: day_col_w,
                height: row_h,
            };

            // Find events that overlap this hour on this date
            let hour_events: Vec<&crate::models::Event> = loaded_events
                .iter()
                .filter(|e| {
                    if !e.occurs_on(date) {
                        return false;
                    }
                    if let Some(start) = e.start_dt() {
                        let start_h = start.hour();
                        let end_h = e.end_dt().map(|en| en.hour()).unwrap_or(start_h + 1);
                        start_h <= hour && hour <= end_h
                    } else {
                        false
                    }
                })
                .copied()
                .collect();

            render_hour_cell(app, frame, col_area, date, hour, &hour_events, hour_is_past);
        }

        y += row_h;
    }
}

fn render_hour_cell(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    _date: NaiveDate,
    hour: u32,
    events: &[&crate::models::Event],
    is_past: bool,
) {
    let t = theme();

    if events.is_empty() {
        // Empty hour: just a faint separator line at the top
        let sep_style = Style::default().fg(Color::Rgb(20, 22, 40));
        let sep = Span::styled("\u{2500}".repeat(area.width as usize), sep_style);
        frame.render_widget(Paragraph::new(Line::from(sep)), Rect { height: 1, ..area });
        return;
    }

    // Render up to `area.width / MIN_EV_WIDTH` stacked events
    let avail_width = area.width;
    let per_event_width = (avail_width / events.len().min(3) as u16).max(4);

    for (i, ev) in events.iter().enumerate().take(3) {
        let cal_idx = app.calendar_index_for(&ev.calendar_id);
        let ev_x = area.x + (i as u16 * per_event_width).min(area.width.saturating_sub(2));
        let ev_w = (per_event_width).min(area.width.saturating_sub(ev_x - area.x + 1));
        if ev_w < 2 {
            break;
        }

        let ev_area = Rect {
            x: ev_x,
            y: area.y,
            width: ev_w,
            height: area.height,
        };

        let rail_style = t.event_rail(cal_idx);
        let title_style = if is_past {
            t.past_dim()
        } else {
            t.event_title()
        };
        let title = truncate(&ev.title, (ev_w as usize).saturating_sub(2));

        // Project badge if applicable
        let has_project = ev.project_id.is_some();

        let mut lines: Vec<Line> = Vec::new();
        // First line: ▌ TITLE
        lines.push(Line::from(vec![
            Span::styled("\u{258c}", rail_style), // ▌
            Span::styled(title, title_style),
        ]));
        // Second line: ▌ time or project badge
        if area.height >= 2 {
            let time_str = if let Some(start) = ev.start_dt() {
                format!("{:02}:{:02}", start.hour(), start.minute())
            } else {
                String::new()
            };
            let second_span = if has_project {
                Span::styled("[P]", t.project_badge())
            } else {
                Span::styled(time_str, t.dimmed())
            };
            lines.push(Line::from(vec![
                Span::styled("\u{258c}", rail_style),
                second_span,
            ]));
        }

        frame.render_widget(Paragraph::new(lines), ev_area);
    }
}
