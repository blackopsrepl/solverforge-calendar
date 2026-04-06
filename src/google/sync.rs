use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::google::auth::GoogleClient;
use crate::models::Calendar;

pub struct CalendarSyncDelta {
    pub events_json: Vec<serde_json::Value>,
    pub new_sync_token: Option<String>,
}

/* Sync a single Google Calendar into the local database. */
/* Returns (events_added, events_updated). */
pub async fn sync_calendar(client: &GoogleClient, calendar: &Calendar) -> Result<(usize, usize)> {
    let conn = crate::db::open()?;
    let sync_token = crate::db::get_sync_token(&conn, &calendar.id)?;
    let delta = fetch_calendar_delta(client, calendar, sync_token.as_deref()).await?;
    apply_calendar_sync(&conn, calendar, delta)
}

pub async fn fetch_calendar_delta(
    client: &GoogleClient,
    calendar: &Calendar,
    sync_token: Option<&str>,
) -> Result<CalendarSyncDelta> {
    let google_cal_id = calendar
        .google_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("calendar '{}' has no google_id", calendar.name))?;

    let access_token = client.refresh_access_token().await?;

    let (events_json, new_sync_token) =
        fetch_events(&access_token, google_cal_id, sync_token).await?;

    Ok(CalendarSyncDelta {
        events_json,
        new_sync_token,
    })
}

pub fn apply_calendar_sync(
    conn: &Connection,
    calendar: &Calendar,
    delta: CalendarSyncDelta,
) -> Result<(usize, usize)> {
    let mut added = 0;
    let mut updated = 0;

    for gev in &delta.events_json {
        // Skip cancelled events (soft-delete them locally)
        if gev["status"].as_str() == Some("cancelled") {
            if let Some(gid) = gev["id"].as_str() {
                // Find local event by google_id and soft-delete it
                let _ = soft_delete_by_google_id(conn, &calendar.id, gid);
            }
            continue;
        }

        match crate::google::types::google_event_to_local(&calendar.id, gev) {
            Ok(event) => {
                // Check if we already have this event (by google_id)
                let exists = event_exists_by_google_id(
                    conn,
                    &event.calendar_id,
                    event.google_id.as_deref().unwrap_or(""),
                )?;
                if exists {
                    let _ = update_event_by_google_id(conn, &event.calendar_id, &event);
                    updated += 1;
                } else {
                    let _ = crate::db::insert_event(conn, &event);
                    added += 1;
                }
            }
            Err(e) => {
                // Log and continue — don't abort entire sync for one bad event
                eprintln!("sync: skipping event: {}", e);
            }
        }
    }

    // Persist new sync token
    if let Some(token) = delta.new_sync_token {
        crate::db::upsert_sync_token(conn, &calendar.id, &token)?;
    }

    Ok((added, updated))
}

async fn fetch_events(
    access_token: &str,
    google_cal_id: &str,
    sync_token: Option<&str>,
) -> Result<(Vec<serde_json::Value>, Option<String>)> {
    let http = reqwest::Client::new();
    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        urlenccode(google_cal_id)
    );

    let mut all_events = Vec::new();
    let mut page_token: Option<String> = None;
    let mut new_sync_token: Option<String> = None;

    loop {
        let mut current_req = http
            .get(&url)
            .bearer_auth(access_token)
            .query(&[("maxResults", "2500"), ("singleEvents", "true")]);

        if let Some(token) = sync_token {
            current_req = current_req.query(&[("syncToken", token)]);
        } else {
            let min_time = (chrono::Utc::now() - chrono::Duration::days(365))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
            current_req = current_req.query(&[("timeMin", min_time)]);
        }

        if let Some(pt) = &page_token {
            current_req = current_req.query(&[("pageToken", pt.as_str())]);
        }

        let resp: serde_json::Value = current_req
            .send()
            .await
            .context("Google Calendar events fetch failed")?
            .json()
            .await
            .context("Google Calendar events JSON parse failed")?;

        if let Some(items) = resp["items"].as_array() {
            all_events.extend(items.clone());
        }

        // Check for next sync token (only on last page)
        if let Some(nst) = resp["nextSyncToken"].as_str() {
            new_sync_token = Some(nst.to_string());
        }

        if let Some(npt) = resp["nextPageToken"].as_str() {
            page_token = Some(npt.to_string());
        } else {
            break;
        }
    }

    Ok((all_events, new_sync_token))
}

fn event_exists_by_google_id(
    conn: &rusqlite::Connection,
    calendar_id: &str,
    google_id: &str,
) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM events
         WHERE calendar_id = ?1
           AND google_id = ?2
           AND deleted_at IS NULL",
        [calendar_id, google_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn update_event_by_google_id(
    conn: &rusqlite::Connection,
    calendar_id: &str,
    event: &crate::models::Event,
) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "UPDATE events SET title=?3, description=?4, location=?5,
                  start_at=?6, end_at=?7, all_day=?8, rrule=?9,
                  google_etag=?10, updated_at=?11
         WHERE calendar_id=?1 AND google_id=?2 AND deleted_at IS NULL",
        rusqlite::params![
            calendar_id,
            event.google_id,
            event.title,
            event.description,
            event.location,
            event.start_at,
            event.end_at,
            event.all_day as i64,
            event.rrule,
            event.google_etag,
            now,
        ],
    )?;
    Ok(())
}

fn soft_delete_by_google_id(
    conn: &rusqlite::Connection,
    calendar_id: &str,
    google_id: &str,
) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "UPDATE events
         SET deleted_at=?3, updated_at=?3
         WHERE calendar_id=?1 AND google_id=?2",
        [calendar_id, google_id, &now],
    )?;
    Ok(())
}

fn urlenccode(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' || c == '@'
            {
                vec![c]
            } else {
                format!("%{:02X}", c as u32).chars().collect()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rusqlite::params;
    use tempfile::TempDir;

    use super::{apply_calendar_sync, CalendarSyncDelta};
    use crate::{
        db,
        models::{Calendar, CalendarSource, Event},
    };

    struct EventSnapshot {
        title: String,
        google_id: Option<String>,
        google_etag: Option<String>,
        deleted_at: Option<String>,
    }

    fn insert_google_calendar(
        conn: &rusqlite::Connection,
        id: &str,
        google_id: &str,
    ) -> Result<Calendar> {
        let calendar = Calendar {
            id: id.to_string(),
            name: format!("Calendar {}", id),
            color: "#50f872".to_string(),
            source: CalendarSource::Google,
            google_id: Some(google_id.to_string()),
            visible: true,
            position: 0,
            created_at: "2026-03-30 10:00:00".to_string(),
            updated_at: "2026-03-30 10:00:00".to_string(),
            deleted_at: None,
        };
        db::insert_calendar(conn, &calendar)?;
        Ok(calendar)
    }

    fn insert_synced_event(
        conn: &rusqlite::Connection,
        id: &str,
        calendar_id: &str,
        google_id: &str,
        title: &str,
    ) -> Result<()> {
        db::insert_event(
            conn,
            &Event {
                id: id.to_string(),
                calendar_id: calendar_id.to_string(),
                project_id: None,
                title: title.to_string(),
                description: None,
                location: None,
                start_at: "2026-03-30 09:00:00".to_string(),
                end_at: "2026-03-30 10:00:00".to_string(),
                all_day: false,
                rrule: None,
                google_id: Some(google_id.to_string()),
                google_etag: Some("\"etag-old\"".to_string()),
                reminder_minutes: None,
                timezone: "UTC".to_string(),
                created_at: "2026-03-30 10:00:00".to_string(),
                updated_at: "2026-03-30 10:00:00".to_string(),
                deleted_at: None,
            },
        )?;
        Ok(())
    }

    fn fetch_event_snapshot(conn: &rusqlite::Connection, event_id: &str) -> Result<EventSnapshot> {
        conn.query_row(
            "SELECT title, google_id, google_etag, deleted_at
             FROM events
             WHERE id = ?1",
            [event_id],
            |row| {
                Ok(EventSnapshot {
                    title: row.get(0)?,
                    google_id: row.get(1)?,
                    google_etag: row.get(2)?,
                    deleted_at: row.get::<_, Option<String>>(3)?,
                })
            },
        )
        .map_err(Into::into)
    }

    #[test]
    fn apply_calendar_sync_uses_supplied_connection() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("calendar.db");
        let conn = db::open_at(&path)?;

        let calendar = Calendar {
            id: "google-cal".to_string(),
            name: "Google".to_string(),
            color: "#50f872".to_string(),
            source: CalendarSource::Google,
            google_id: Some("google-cal-id".to_string()),
            visible: true,
            position: 0,
            created_at: "2026-03-30 10:00:00".to_string(),
            updated_at: "2026-03-30 10:00:00".to_string(),
            deleted_at: None,
        };
        db::insert_calendar(&conn, &calendar)?;

        let delta = CalendarSyncDelta {
            events_json: vec![serde_json::json!({
                "id": "google-event-1",
                "summary": "Imported event",
                "etag": "\"etag-1\"",
                "status": "confirmed",
                "start": {
                    "dateTime": "2026-03-30T09:00:00Z",
                    "timeZone": "UTC"
                },
                "end": {
                    "dateTime": "2026-03-30T10:00:00Z"
                }
            })],
            new_sync_token: Some("sync-token-1".to_string()),
        };

        let (added, updated) = apply_calendar_sync(&conn, &calendar, delta)?;
        assert_eq!(added, 1);
        assert_eq!(updated, 0);

        let events = db::load_events(&conn)?;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].calendar_id, calendar.id);
        assert_eq!(events[0].google_id.as_deref(), Some("google-event-1"));
        assert_eq!(
            db::get_sync_token(&conn, &calendar.id)?.as_deref(),
            Some("sync-token-1")
        );

        Ok(())
    }

    #[test]
    fn apply_calendar_sync_inserts_even_when_other_calendar_has_same_google_id() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("calendar.db");
        let conn = db::open_at(&path)?;
        let retained = insert_google_calendar(&conn, "google-cal-a", "google-cal-a-id")?;
        let other = insert_google_calendar(&conn, "google-cal-b", "google-cal-b-id")?;
        insert_synced_event(
            &conn,
            "other-event",
            &other.id,
            "shared-google-event",
            "Other calendar event",
        )?;

        let delta = CalendarSyncDelta {
            events_json: vec![serde_json::json!({
                "id": "shared-google-event",
                "summary": "Imported into retained calendar",
                "etag": "\"etag-a\"",
                "status": "confirmed",
                "start": {
                    "dateTime": "2026-03-31T09:00:00Z",
                    "timeZone": "UTC"
                },
                "end": {
                    "dateTime": "2026-03-31T10:00:00Z"
                }
            })],
            new_sync_token: None,
        };

        let (added, updated) = apply_calendar_sync(&conn, &retained, delta)?;
        assert_eq!(added, 1);
        assert_eq!(updated, 0);

        let matching_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM events WHERE calendar_id = ?1 AND google_id = ?2 AND deleted_at IS NULL",
            params![retained.id, "shared-google-event"],
            |row| row.get(0),
        )?;
        assert_eq!(matching_count, 1);

        Ok(())
    }

    #[test]
    fn apply_calendar_sync_updates_only_matching_calendar_event() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("calendar.db");
        let conn = db::open_at(&path)?;
        let retained = insert_google_calendar(&conn, "google-cal-a", "google-cal-a-id")?;
        let other = insert_google_calendar(&conn, "google-cal-b", "google-cal-b-id")?;
        insert_synced_event(
            &conn,
            "retained-event",
            &retained.id,
            "shared-google-event",
            "Old A",
        )?;
        insert_synced_event(
            &conn,
            "other-event",
            &other.id,
            "shared-google-event",
            "Old B",
        )?;

        let delta = CalendarSyncDelta {
            events_json: vec![serde_json::json!({
                "id": "shared-google-event",
                "summary": "Updated A",
                "etag": "\"etag-a-new\"",
                "status": "confirmed",
                "start": {
                    "dateTime": "2026-04-01T09:00:00Z",
                    "timeZone": "UTC"
                },
                "end": {
                    "dateTime": "2026-04-01T10:00:00Z"
                }
            })],
            new_sync_token: None,
        };

        let (added, updated) = apply_calendar_sync(&conn, &retained, delta)?;
        assert_eq!(added, 0);
        assert_eq!(updated, 1);

        let retained_event = fetch_event_snapshot(&conn, "retained-event")?;
        let other_event = fetch_event_snapshot(&conn, "other-event")?;
        assert_eq!(retained_event.title, "Updated A");
        assert_eq!(
            retained_event.google_id.as_deref(),
            Some("shared-google-event")
        );
        assert_eq!(
            retained_event.google_etag.as_deref(),
            Some("\"etag-a-new\"")
        );
        assert_eq!(other_event.title, "Old B");
        assert_eq!(
            other_event.google_id.as_deref(),
            Some("shared-google-event")
        );
        assert_eq!(other_event.google_etag.as_deref(), Some("\"etag-old\""));

        Ok(())
    }

    #[test]
    fn apply_calendar_sync_soft_deletes_only_matching_calendar_event() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("calendar.db");
        let conn = db::open_at(&path)?;
        let retained = insert_google_calendar(&conn, "google-cal-a", "google-cal-a-id")?;
        let other = insert_google_calendar(&conn, "google-cal-b", "google-cal-b-id")?;
        insert_synced_event(
            &conn,
            "retained-event",
            &retained.id,
            "shared-google-event",
            "Old A",
        )?;
        insert_synced_event(
            &conn,
            "other-event",
            &other.id,
            "shared-google-event",
            "Old B",
        )?;

        let delta = CalendarSyncDelta {
            events_json: vec![serde_json::json!({
                "id": "shared-google-event",
                "status": "cancelled"
            })],
            new_sync_token: None,
        };

        let (added, updated) = apply_calendar_sync(&conn, &retained, delta)?;
        assert_eq!(added, 0);
        assert_eq!(updated, 0);

        let retained_event = fetch_event_snapshot(&conn, "retained-event")?;
        let other_event = fetch_event_snapshot(&conn, "other-event")?;
        assert!(retained_event.deleted_at.is_some());
        assert!(other_event.deleted_at.is_none());

        Ok(())
    }
}
