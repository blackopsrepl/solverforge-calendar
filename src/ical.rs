/* iCal (.ics) import and export using the `icalendar` crate.  */

use anyhow::{Context, Result};
use chrono::Utc;
use icalendar::{
    Calendar as ICalCalendar, CalendarComponent, Component, Event as ICalEvent, EventLike,
};
use uuid::Uuid;

use crate::models::Event;

// ── Export ────────────────────────────────────────────────────────────

/* Export a list of events to an iCal string. */
pub fn export_events(events: &[Event], calendar_name: &str) -> String {
    let mut cal = ICalCalendar::new();
    cal.name(calendar_name);
    cal.description("Exported from SolverForge Calendar");

    for ev in events {
        let mut ical_event = ICalEvent::new();
        ical_event.uid(&ev.id);
        ical_event.summary(&ev.title);

        if let Some(desc) = &ev.description {
            ical_event.description(desc);
        }
        if let Some(loc) = &ev.location {
            ical_event.location(loc);
        }

        // Timestamps
        if ev.all_day {
            if let Ok(date) = chrono::NaiveDate::parse_from_str(&ev.start_at[..10], "%Y-%m-%d") {
                ical_event.add_property("DTSTART;VALUE=DATE", &date.format("%Y%m%d").to_string());
            }
            if let Ok(date) = chrono::NaiveDate::parse_from_str(&ev.end_at[..10], "%Y-%m-%d") {
                ical_event.add_property("DTEND;VALUE=DATE", &date.format("%Y%m%d").to_string());
            }
        } else {
            if let Some(dt) = ev.start_dt() {
                ical_event.starts(dt);
            }
            if let Some(dt) = ev.end_dt() {
                ical_event.ends(dt);
            }
        }

        // RRULE
        if let Some(rrule) = &ev.rrule {
            ical_event.add_property("RRULE", rrule.strip_prefix("RRULE:").unwrap_or(rrule));
        }

        cal.push(ical_event.done());
    }

    cal.to_string()
}

/* Write events to an .ics file. */
pub fn export_to_file(events: &[Event], calendar_name: &str, path: &std::path::Path) -> Result<()> {
    let content = export_events(events, calendar_name);
    std::fs::write(path, content)
        .with_context(|| format!("cannot write iCal to {}", path.display()))
}

// ── Import ────────────────────────────────────────────────────────────

/* Parse an .ics string and return a list of Events (not yet saved to DB). */
pub fn import_from_str(content: &str, default_calendar_id: &str) -> Result<Vec<Event>> {
    let cal: ICalCalendar = content
        .parse()
        .map_err(|e: String| anyhow::anyhow!("iCal parse error: {}", e))?;

    let mut events = Vec::new();
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    for component in cal.components {
        let ical_ev = match component {
            CalendarComponent::Event(e) => e,
            _ => continue,
        };

        let title = ical_ev.get_summary().unwrap_or("(no title)").to_string();
        let id = ical_ev
            .get_uid()
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        // Parse start/end from raw properties (robust approach)
        let (start_at, all_day) = parse_prop_datetime(&ical_ev, "DTSTART");
        let (end_at, _) = parse_prop_datetime(&ical_ev, "DTEND");

        let rrule = ical_ev
            .property_value("RRULE")
            .map(|v| format!("RRULE:{}", v));

        events.push(Event {
            id,
            calendar_id: default_calendar_id.to_string(),
            project_id: None,
            title,
            description: ical_ev.get_description().map(|s| s.to_string()),
            location: ical_ev.get_location().map(|s| s.to_string()),
            start_at,
            end_at,
            all_day,
            rrule,
            google_id: None,
            google_etag: None,
            reminder_minutes: None,
            timezone: "UTC".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            deleted_at: None,
        });
    }

    Ok(events)
}

pub fn import_from_file(path: &std::path::Path, default_calendar_id: &str) -> Result<Vec<Event>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read iCal file: {}", path.display()))?;
    import_from_str(&content, default_calendar_id)
}

fn parse_prop_datetime(ev: &ICalEvent, prop: &str) -> (String, bool) {
    let fallback = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let val = match ev.property_value(prop) {
        Some(v) => v.to_string(),
        None => return (fallback, false),
    };

    // All-day: 8 digits like 20260217
    if val.len() == 8 && val.chars().all(|c| c.is_ascii_digit()) {
        return (
            format!("{}-{}-{} 00:00:00", &val[..4], &val[4..6], &val[6..8]),
            true,
        );
    }
    // DateTime with T separator: 20260217T090000Z
    if val.len() >= 15 && val.chars().nth(8) == Some('T') {
        let d = &val[..8];
        let t = &val[9..15];
        return (
            format!(
                "{}-{}-{} {}:{}:{}",
                &d[..4],
                &d[4..6],
                &d[6..8],
                &t[..2],
                &t[2..4],
                &t[4..6]
            ),
            false,
        );
    }

    (fallback, false)
}
