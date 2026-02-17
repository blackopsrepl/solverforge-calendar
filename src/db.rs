use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::Connection;

const SCHEMA_VERSION: &str = "20260101000001";

/* Path to the calendar database. */
pub fn db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("solverforge")
        .join("calendar.db")
}

/* Open (or create) the database and run pending migrations. */
pub fn open() -> Result<Connection> {
    let path = db_path();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create data directory: {}", parent.display()))?;
    }

    let conn = Connection::open(&path)
        .with_context(|| format!("cannot open database: {}", path.display()))?;

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA synchronous = NORMAL;",
    )
    .context("cannot configure pragmas")?;

    migrate(&conn).context("schema migration failed")?;

    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    // Rails-compatible schema_migrations table.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version TEXT PRIMARY KEY
        );",
    )?;

    let applied: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
            [SCHEMA_VERSION],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    if !applied {
        migrate_v1(conn)?;
        conn.execute(
            "INSERT OR IGNORE INTO schema_migrations (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )?;
    }

    Ok(())
}

fn migrate_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        -- ── calendars ────────────────────────────────────────────────
        CREATE TABLE IF NOT EXISTS calendars (
            id          TEXT PRIMARY KEY,          -- UUID v4
            name        TEXT    NOT NULL,
            color       TEXT    NOT NULL,          -- hex e.g. '#50f872'
            source      TEXT    NOT NULL DEFAULT 'local',  -- 'local' | 'google'
            google_id   TEXT,                      -- Google Calendar ID (for synced calendars)
            visible     INTEGER NOT NULL DEFAULT 1,
            position    INTEGER NOT NULL DEFAULT 0,
            created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
            updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
            deleted_at  TEXT
        );

        -- ── projects ─────────────────────────────────────────────────
        CREATE TABLE IF NOT EXISTS projects (
            id          TEXT PRIMARY KEY,
            name        TEXT    NOT NULL,
            color       TEXT    NOT NULL,
            description TEXT,
            created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
            updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
            deleted_at  TEXT
        );

        -- ── events ───────────────────────────────────────────────────
        CREATE TABLE IF NOT EXISTS events (
            id               TEXT PRIMARY KEY,
            calendar_id      TEXT    NOT NULL REFERENCES calendars(id),
            project_id       TEXT             REFERENCES projects(id),
            title            TEXT    NOT NULL,
            description      TEXT,
            location         TEXT,
            start_at         TEXT    NOT NULL,  -- ISO 8601 e.g. '2026-02-17 09:00:00'
            end_at           TEXT    NOT NULL,
            all_day          INTEGER NOT NULL DEFAULT 0,
            rrule            TEXT,              -- RFC 5545 RRULE string (recurring events)
            google_id        TEXT,              -- Google Event ID
            google_etag      TEXT,              -- Google ETag for conflict detection
            reminder_minutes INTEGER,           -- minutes before event; NULL = no reminder
            timezone         TEXT    NOT NULL DEFAULT 'UTC',
            created_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
            updated_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
            deleted_at       TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_events_calendar_id ON events(calendar_id);
        CREATE INDEX IF NOT EXISTS idx_events_project_id  ON events(project_id);
        CREATE INDEX IF NOT EXISTS idx_events_start_at    ON events(start_at);
        CREATE INDEX IF NOT EXISTS idx_events_end_at      ON events(end_at);
        CREATE INDEX IF NOT EXISTS idx_events_google_id   ON events(google_id)
            WHERE google_id IS NOT NULL;

        -- ── event_dependencies ───────────────────────────────────────
        -- DAG edges: from_event_id blocks to_event_id.
        -- Rails model: EventDependency
        CREATE TABLE IF NOT EXISTS event_dependencies (
            id              TEXT PRIMARY KEY,
            from_event_id   TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            to_event_id     TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            dependency_type TEXT NOT NULL DEFAULT 'blocks',  -- 'blocks' | 'related'
            created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
            updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
            UNIQUE(from_event_id, to_event_id)
        );

        CREATE INDEX IF NOT EXISTS idx_event_deps_from ON event_dependencies(from_event_id);
        CREATE INDEX IF NOT EXISTS idx_event_deps_to   ON event_dependencies(to_event_id);

        -- ── recurrence_exceptions ─────────────────────────────────────
        -- Modifies or deletes a single occurrence of a recurring event.
        CREATE TABLE IF NOT EXISTS recurrence_exceptions (
            id                   TEXT PRIMARY KEY,
            event_id             TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            original_start       TEXT NOT NULL,     -- datetime of the overridden occurrence
            replacement_event_id TEXT REFERENCES events(id),  -- NULL = deleted occurrence
            created_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
            updated_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now'))
        );

        CREATE INDEX IF NOT EXISTS idx_recurrence_exc_event_id
            ON recurrence_exceptions(event_id);

        -- ── sync_tokens ──────────────────────────────────────────────
        -- Google Calendar incremental sync state per calendar.
        CREATE TABLE IF NOT EXISTS sync_tokens (
            id          TEXT PRIMARY KEY,
            calendar_id TEXT NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
            sync_token  TEXT NOT NULL,
            synced_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
            UNIQUE(calendar_id)
        );
        ",
    )?;

    // Seed a default local calendar if the table is empty.
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM calendars", [], |row| row.get(0))?;

    if count == 0 {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO calendars (id, name, color, source, position)
             VALUES (?1, 'Personal', '#82FB9C', 'local', 0)",
            [&id],
        )?;
    }

    Ok(())
}

// ── CRUD helpers ─────────────────────────────────────────────────────

use crate::models::{Calendar, CalendarSource, DependencyType, Event, EventDependency, Project};

pub fn load_calendars(conn: &Connection) -> Result<Vec<Calendar>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, color, source, google_id, visible, position,
                created_at, updated_at, deleted_at
         FROM calendars
         WHERE deleted_at IS NULL
         ORDER BY position, name",
    )?;
    let rows = stmt.query_map([], |row| {
        let source_str: String = row.get(3)?;
        let source = if source_str == "google" {
            CalendarSource::Google
        } else {
            CalendarSource::Local
        };
        Ok(Calendar {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
            source,
            google_id: row.get(4)?,
            visible: row.get::<_, i64>(5)? != 0,
            position: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
            deleted_at: row.get(9)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn load_projects(conn: &Connection) -> Result<Vec<Project>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, color, description, created_at, updated_at, deleted_at
         FROM projects WHERE deleted_at IS NULL ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Project {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
            description: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
            deleted_at: row.get(6)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

/* Load events within a datetime window (inclusive). Excludes soft-deleted. */
pub fn load_events_in_range(conn: &Connection, from: &str, to: &str) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT id, calendar_id, project_id, title, description, location,
                start_at, end_at, all_day, rrule, google_id, google_etag,
                reminder_minutes, timezone, created_at, updated_at, deleted_at
         FROM events
         WHERE deleted_at IS NULL
           AND start_at <= ?2 AND end_at >= ?1
         ORDER BY start_at",
    )?;
    query_events(&mut stmt, &[from, to])
}

fn query_events(stmt: &mut rusqlite::Statement, params: &[&str]) -> Result<Vec<Event>> {
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
        Ok(Event {
            id: row.get(0)?,
            calendar_id: row.get(1)?,
            project_id: row.get(2)?,
            title: row.get(3)?,
            description: row.get(4)?,
            location: row.get(5)?,
            start_at: row.get(6)?,
            end_at: row.get(7)?,
            all_day: row.get::<_, i64>(8)? != 0,
            rrule: row.get(9)?,
            google_id: row.get(10)?,
            google_etag: row.get(11)?,
            reminder_minutes: row.get(12)?,
            timezone: row.get(13)?,
            created_at: row.get(14)?,
            updated_at: row.get(15)?,
            deleted_at: row.get(16)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn insert_event(conn: &Connection, ev: &Event) -> Result<()> {
    conn.execute(
        "INSERT INTO events
             (id, calendar_id, project_id, title, description, location,
              start_at, end_at, all_day, rrule, google_id, google_etag,
              reminder_minutes, timezone, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        rusqlite::params![
            ev.id,
            ev.calendar_id,
            ev.project_id,
            ev.title,
            ev.description,
            ev.location,
            ev.start_at,
            ev.end_at,
            ev.all_day as i64,
            ev.rrule,
            ev.google_id,
            ev.google_etag,
            ev.reminder_minutes,
            ev.timezone,
            ev.created_at,
            ev.updated_at,
        ],
    )?;
    Ok(())
}

pub fn update_event(conn: &Connection, ev: &Event) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "UPDATE events SET
             calendar_id=?2, project_id=?3, title=?4, description=?5, location=?6,
             start_at=?7, end_at=?8, all_day=?9, rrule=?10, reminder_minutes=?11,
             timezone=?12, updated_at=?13
         WHERE id=?1",
        rusqlite::params![
            ev.id,
            ev.calendar_id,
            ev.project_id,
            ev.title,
            ev.description,
            ev.location,
            ev.start_at,
            ev.end_at,
            ev.all_day as i64,
            ev.rrule,
            ev.reminder_minutes,
            ev.timezone,
            now,
        ],
    )?;
    Ok(())
}

/* Soft-delete an event by setting deleted_at. */
pub fn soft_delete_event(conn: &Connection, event_id: &str) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "UPDATE events SET deleted_at=?2, updated_at=?2 WHERE id=?1",
        [event_id, &now],
    )?;
    Ok(())
}

pub fn load_dependencies(conn: &Connection) -> Result<Vec<EventDependency>> {
    let mut stmt = conn.prepare(
        "SELECT id, from_event_id, to_event_id, dependency_type, created_at, updated_at
         FROM event_dependencies",
    )?;
    let rows = stmt.query_map([], |row| {
        let dep_type_str: String = row.get(3)?;
        let dependency_type = if dep_type_str == "blocks" {
            DependencyType::Blocks
        } else {
            DependencyType::Related
        };
        Ok(EventDependency {
            id: row.get(0)?,
            from_event_id: row.get(1)?,
            to_event_id: row.get(2)?,
            dependency_type,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn upsert_sync_token(conn: &Connection, calendar_id: &str, token: &str) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "INSERT INTO sync_tokens (id, calendar_id, sync_token, synced_at)
         VALUES (?1,?2,?3,?4)
         ON CONFLICT(calendar_id) DO UPDATE SET sync_token=?3, synced_at=?4",
        rusqlite::params![id, calendar_id, token, now],
    )?;
    Ok(())
}

pub fn get_sync_token(conn: &Connection, calendar_id: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT sync_token FROM sync_tokens WHERE calendar_id=?1",
        [calendar_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

// Re-export rusqlite Optional trait for our use of .optional()
trait OptionalExt<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}
impl<T> OptionalExt<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
