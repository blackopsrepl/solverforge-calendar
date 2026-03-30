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

    let access_token = refresh_access_token(client).await?;

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
                let _ = soft_delete_by_google_id(conn, gid);
            }
            continue;
        }

        match crate::google::types::google_event_to_local(&calendar.id, gev) {
            Ok(event) => {
                // Check if we already have this event (by google_id)
                let exists =
                    event_exists_by_google_id(conn, event.google_id.as_deref().unwrap_or(""))?;
                if exists {
                    let _ = update_event_by_google_id(conn, &event);
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

async fn refresh_access_token(client: &GoogleClient) -> Result<String> {
    let http = reqwest::Client::new();
    let params = [
        ("client_id", client.client_id.as_str()),
        ("client_secret", client.client_secret.as_str()),
        ("refresh_token", client.refresh_token.as_str()),
        ("grant_type", "refresh_token"),
    ];
    let resp: serde_json::Value = http
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .context("token refresh request failed")?
        .json()
        .await
        .context("token refresh JSON parse failed")?;

    resp["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("no access_token in refresh response: {:?}", resp))
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

fn event_exists_by_google_id(conn: &rusqlite::Connection, google_id: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM events WHERE google_id = ?1 AND deleted_at IS NULL",
        [google_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn update_event_by_google_id(
    conn: &rusqlite::Connection,
    event: &crate::models::Event,
) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "UPDATE events SET title=?2, description=?3, location=?4,
                  start_at=?5, end_at=?6, all_day=?7, rrule=?8,
                  google_etag=?9, updated_at=?10
         WHERE google_id=?1 AND deleted_at IS NULL",
        rusqlite::params![
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

fn soft_delete_by_google_id(conn: &rusqlite::Connection, google_id: &str) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "UPDATE events SET deleted_at=?2, updated_at=?2 WHERE google_id=?1",
        [google_id, &now],
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
    use tempfile::TempDir;

    use super::{apply_calendar_sync, CalendarSyncDelta};
    use crate::{
        db,
        models::{Calendar, CalendarSource},
    };

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
}
