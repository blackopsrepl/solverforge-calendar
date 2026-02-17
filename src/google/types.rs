use crate::models::{Calendar, CalendarSource, Event};
use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

pub fn google_calendar_to_local(google_id: &str, name: &str, color_hex: &str) -> Calendar {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    Calendar {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        color: color_hex.to_string(),
        source: CalendarSource::Google,
        google_id: Some(google_id.to_string()),
        visible: true,
        position: 99,
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    }
}

pub fn google_event_to_local(calendar_id: &str, gev: &serde_json::Value) -> Result<Event> {
    let google_id = gev["id"].as_str().unwrap_or("").to_string();
    let title = gev["summary"].as_str().unwrap_or("(no title)").to_string();
    let description = gev["description"].as_str().map(|s| s.to_string());
    let location = gev["location"].as_str().map(|s| s.to_string());
    let google_etag = gev["etag"].as_str().map(|s| s.to_string());

    // Determine timezone
    let timezone = gev["start"]["timeZone"]
        .as_str()
        .unwrap_or("UTC")
        .to_string();

    // Parse start/end (dateTime or date for all-day events)
    let (start_at, all_day) = if let Some(dt) = gev["start"]["dateTime"].as_str() {
        (parse_google_datetime(dt)?, false)
    } else if let Some(d) = gev["start"]["date"].as_str() {
        (format!("{} 00:00:00", d), true)
    } else {
        return Err(anyhow::anyhow!("event has no start time"));
    };

    let end_at = if let Some(dt) = gev["end"]["dateTime"].as_str() {
        parse_google_datetime(dt)?
    } else if let Some(d) = gev["end"]["date"].as_str() {
        format!("{} 00:00:00", d)
    } else {
        return Err(anyhow::anyhow!("event has no end time"));
    };

    // Extract RRULE if present
    let rrule = gev["recurrence"]
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|v| v.as_str().map(|s| s.starts_with("RRULE:")).unwrap_or(false))
        })
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    Ok(Event {
        id: Uuid::new_v4().to_string(),
        calendar_id: calendar_id.to_string(),
        project_id: None,
        title,
        description,
        location,
        start_at,
        end_at,
        all_day,
        rrule,
        google_id: Some(google_id),
        google_etag,
        reminder_minutes: None,
        timezone,
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    })
}

/* Parse a Google RFC 3339 datetime string to our storage format. */
fn parse_google_datetime(s: &str) -> Result<String> {
    // Try parsing as RFC 3339
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt
            .with_timezone(&Utc)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string());
    }
    // Fallback: already in our format
    Ok(s.to_string())
}

/* Google Calendar API color ID to hex mapping. */
pub fn google_color_id_to_hex(color_id: &str) -> &'static str {
    match color_id {
        "1" => "#7986CB",  // Lavender
        "2" => "#33B679",  // Sage
        "3" => "#8E24AA",  // Grape
        "4" => "#E67C73",  // Flamingo
        "5" => "#F6BF26",  // Banana
        "6" => "#F4511E",  // Tangerine
        "7" => "#039BE5",  // Peacock
        "8" => "#616161",  // Graphite
        "9" => "#3F51B5",  // Blueberry
        "10" => "#0B8043", // Basil
        "11" => "#D50000", // Tomato
        _ => "#82FB9C",    // fallback: accent green
    }
}
