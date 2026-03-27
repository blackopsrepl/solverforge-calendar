use std::collections::HashMap;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use solverforge_calendar::{db, google, models};
use uuid::Uuid;

fn main() {
    if let Err(err) = run() {
        eprintln!(
            "{{\"status\":\"error\",\"message\":{}}}",
            serde_json::to_string(&err.to_string()).unwrap()
        );
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help" | "help") {
        print_help();
        return Ok(());
    }

    let resource = args.remove(0);
    let action = args
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("missing action for resource '{}'", resource))?;
    args.remove(0);

    let conn = db::open()?;

    match resource.as_str() {
        "calendars" => handle_calendars(&conn, &action, args),
        "projects" => handle_projects(&conn, &action, args),
        "events" => handle_events(&conn, &action, args),
        "dependencies" => handle_dependencies(&conn, &action, args),
        "google" => handle_google(&conn, &action, args),
        other => bail!(
            "unknown resource '{}'; expected calendars, projects, events, dependencies, or google",
            other
        ),
    }
}

fn handle_calendars(conn: &rusqlite::Connection, action: &str, args: Vec<String>) -> Result<()> {
    match action {
        "list" => print_json(&db::load_calendars(conn)?),
        "get" => {
            let id = expect_single_id(args, "calendar")?;
            print_json(&require_resource(
                db::get_calendar(conn, &id)?,
                "calendar",
                &id,
            )?)
        }
        "create" => {
            let flags = parse_flags(args)?;
            let source =
                parse_calendar_source(flags.get("source").map(String::as_str).unwrap_or("local"))?;
            let google_id = optional_string(&flags, "google-id");
            if matches!(source, models::CalendarSource::Google) && google_id.is_none() {
                bail!("google calendars require --google-id")
            }
            let now = now_timestamp();
            let calendar = models::Calendar {
                id: Uuid::new_v4().to_string(),
                name: required_string(&flags, "name")?,
                color: required_string(&flags, "color")?,
                source,
                google_id,
                visible: parse_bool_flag(&flags, "visible")?.unwrap_or(true),
                position: parse_i64_flag(&flags, "position")?.unwrap_or(0),
                created_at: now.clone(),
                updated_at: now,
                deleted_at: None,
            };
            db::insert_calendar(conn, &calendar)?;
            print_json(&calendar)
        }
        "update" => {
            let (id, flags) = split_id_and_flags(args, "calendar")?;
            let mut calendar = require_resource(db::get_calendar(conn, &id)?, "calendar", &id)?;
            if let Some(name) = optional_string(&flags, "name") {
                calendar.name = name;
            }
            if let Some(color) = optional_string(&flags, "color") {
                calendar.color = color;
            }
            if let Some(source) = flags.get("source") {
                calendar.source = parse_calendar_source(source)?;
            }
            if flags.contains_key("google-id") {
                calendar.google_id = optional_string(&flags, "google-id");
            }
            if let Some(visible) = parse_bool_flag(&flags, "visible")? {
                calendar.visible = visible;
            }
            if let Some(position) = parse_i64_flag(&flags, "position")? {
                calendar.position = position;
            }
            if matches!(calendar.source, models::CalendarSource::Google)
                && calendar.google_id.is_none()
            {
                bail!("google calendars require --google-id")
            }
            db::update_calendar(conn, &calendar)?;
            print_json(&require_resource(
                db::get_calendar(conn, &id)?,
                "calendar",
                &id,
            )?)
        }
        "delete" => {
            let id = expect_single_id(args, "calendar")?;
            require_resource(db::get_calendar(conn, &id)?, "calendar", &id)?;
            db::soft_delete_calendar(conn, &id)?;
            print_deleted("calendar", &id)
        }
        other => bail!("unknown calendars action '{}'", other),
    }
}

fn handle_projects(conn: &rusqlite::Connection, action: &str, args: Vec<String>) -> Result<()> {
    match action {
        "list" => print_json(&db::load_projects(conn)?),
        "get" => {
            let id = expect_single_id(args, "project")?;
            print_json(&require_resource(
                db::get_project(conn, &id)?,
                "project",
                &id,
            )?)
        }
        "create" => {
            let flags = parse_flags(args)?;
            let now = now_timestamp();
            let project = models::Project {
                id: Uuid::new_v4().to_string(),
                name: required_string(&flags, "name")?,
                color: required_string(&flags, "color")?,
                description: optional_string(&flags, "description"),
                created_at: now.clone(),
                updated_at: now,
                deleted_at: None,
            };
            db::insert_project(conn, &project)?;
            print_json(&project)
        }
        "update" => {
            let (id, flags) = split_id_and_flags(args, "project")?;
            let mut project = require_resource(db::get_project(conn, &id)?, "project", &id)?;
            if let Some(name) = optional_string(&flags, "name") {
                project.name = name;
            }
            if let Some(color) = optional_string(&flags, "color") {
                project.color = color;
            }
            if flags.contains_key("description") {
                project.description = optional_string(&flags, "description");
            }
            db::update_project(conn, &project)?;
            print_json(&require_resource(
                db::get_project(conn, &id)?,
                "project",
                &id,
            )?)
        }
        "delete" => {
            let id = expect_single_id(args, "project")?;
            require_resource(db::get_project(conn, &id)?, "project", &id)?;
            db::soft_delete_project(conn, &id)?;
            print_deleted("project", &id)
        }
        other => bail!("unknown projects action '{}'", other),
    }
}

fn handle_events(conn: &rusqlite::Connection, action: &str, args: Vec<String>) -> Result<()> {
    match action {
        "list" => {
            let flags = parse_flags(args)?;
            let events = match (
                optional_string(&flags, "from"),
                optional_string(&flags, "to"),
            ) {
                (Some(from), Some(to)) => db::load_events_in_range(conn, &from, &to)?,
                (None, None) => db::load_events(conn)?,
                _ => bail!("--from and --to must be supplied together for event range queries"),
            };
            print_json(&events)
        }
        "get" => {
            let id = expect_single_id(args, "event")?;
            print_json(&require_resource(db::get_event(conn, &id)?, "event", &id)?)
        }
        "create" => {
            let flags = parse_flags(args)?;
            let calendar_id = required_string(&flags, "calendar-id")?;
            require_resource(
                db::get_calendar(conn, &calendar_id)?,
                "calendar",
                &calendar_id,
            )
            .context("calendar-id must reference an existing non-deleted calendar")?;
            let project_id = optional_string(&flags, "project-id");
            if let Some(project_id_value) = project_id.as_deref() {
                require_resource(
                    db::get_project(conn, project_id_value)?,
                    "project",
                    project_id_value,
                )
                .context("project-id must reference an existing non-deleted project")?;
            }
            let start_at = required_string(&flags, "start-at")?;
            let end_at = required_string(&flags, "end-at")?;
            validate_event_datetime(&start_at, &end_at)?;
            let now = now_timestamp();
            let event = models::Event {
                id: Uuid::new_v4().to_string(),
                calendar_id,
                project_id,
                title: required_string(&flags, "title")?,
                description: optional_string(&flags, "description"),
                location: optional_string(&flags, "location"),
                start_at,
                end_at,
                all_day: parse_bool_flag(&flags, "all-day")?.unwrap_or(false),
                rrule: optional_string(&flags, "rrule"),
                google_id: None,
                google_etag: None,
                reminder_minutes: parse_i64_flag(&flags, "reminder-minutes")?,
                timezone: string_or_default(&flags, "timezone", "UTC"),
                created_at: now.clone(),
                updated_at: now,
                deleted_at: None,
            };
            db::insert_event(conn, &event)?;
            print_json(&event)
        }
        "update" => {
            let (id, flags) = split_id_and_flags(args, "event")?;
            let mut event = require_resource(db::get_event(conn, &id)?, "event", &id)?;
            if let Some(calendar_id) = optional_string(&flags, "calendar-id") {
                require_resource(
                    db::get_calendar(conn, &calendar_id)?,
                    "calendar",
                    &calendar_id,
                )
                .context("calendar-id must reference an existing non-deleted calendar")?;
                event.calendar_id = calendar_id;
            }
            if flags.contains_key("project-id") {
                event.project_id = optional_string(&flags, "project-id");
                if let Some(project_id) = event.project_id.as_deref() {
                    require_resource(db::get_project(conn, project_id)?, "project", project_id)
                        .context("project-id must reference an existing non-deleted project")?;
                }
            }
            if let Some(title) = optional_string(&flags, "title") {
                event.title = title;
            }
            if flags.contains_key("description") {
                event.description = optional_string(&flags, "description");
            }
            if flags.contains_key("location") {
                event.location = optional_string(&flags, "location");
            }
            if let Some(start_at) = optional_string(&flags, "start-at") {
                event.start_at = start_at;
            }
            if let Some(end_at) = optional_string(&flags, "end-at") {
                event.end_at = end_at;
            }
            if let Some(timezone) = optional_string(&flags, "timezone") {
                event.timezone = timezone;
            }
            if let Some(all_day) = parse_bool_flag(&flags, "all-day")? {
                event.all_day = all_day;
            }
            if flags.contains_key("rrule") {
                event.rrule = optional_string(&flags, "rrule");
            }
            if flags.contains_key("reminder-minutes") {
                event.reminder_minutes = parse_i64_flag(&flags, "reminder-minutes")?;
            }
            validate_event_datetime(&event.start_at, &event.end_at)?;
            db::update_event(conn, &event)?;
            print_json(&require_resource(db::get_event(conn, &id)?, "event", &id)?)
        }
        "delete" => {
            let id = expect_single_id(args, "event")?;
            require_resource(db::get_event(conn, &id)?, "event", &id)?;
            db::soft_delete_event(conn, &id)?;
            print_deleted("event", &id)
        }
        other => bail!("unknown events action '{}'", other),
    }
}

fn handle_dependencies(conn: &rusqlite::Connection, action: &str, args: Vec<String>) -> Result<()> {
    match action {
        "list" => print_json(&db::load_dependencies(conn)?),
        "get" => {
            let id = expect_single_id(args, "dependency")?;
            print_json(&require_resource(
                db::get_dependency(conn, &id)?,
                "dependency",
                &id,
            )?)
        }
        "create" => {
            let flags = parse_flags(args)?;
            let from_event_id = required_string(&flags, "from-event-id")?;
            let to_event_id = required_string(&flags, "to-event-id")?;
            validate_dependency(conn, &from_event_id, &to_event_id)?;
            let now = now_timestamp();
            let dependency = models::EventDependency {
                id: Uuid::new_v4().to_string(),
                from_event_id,
                to_event_id,
                dependency_type: parse_dependency_type(
                    flags
                        .get("dependency-type")
                        .map(String::as_str)
                        .unwrap_or("blocks"),
                )?,
                created_at: now.clone(),
                updated_at: now,
            };
            db::insert_dependency(conn, &dependency)?;
            print_json(&dependency)
        }
        "update" => {
            let (id, flags) = split_id_and_flags(args, "dependency")?;
            let mut dependency =
                require_resource(db::get_dependency(conn, &id)?, "dependency", &id)?;
            if let Some(from_event_id) = optional_string(&flags, "from-event-id") {
                dependency.from_event_id = from_event_id;
            }
            if let Some(to_event_id) = optional_string(&flags, "to-event-id") {
                dependency.to_event_id = to_event_id;
            }
            if let Some(dependency_type) = optional_string(&flags, "dependency-type") {
                dependency.dependency_type = parse_dependency_type(&dependency_type)?;
            }
            validate_dependency(conn, &dependency.from_event_id, &dependency.to_event_id)?;
            db::update_dependency(conn, &dependency)?;
            print_json(&require_resource(
                db::get_dependency(conn, &id)?,
                "dependency",
                &id,
            )?)
        }
        "delete" => {
            let id = expect_single_id(args, "dependency")?;
            require_resource(db::get_dependency(conn, &id)?, "dependency", &id)?;
            db::delete_dependency(conn, &id)?;
            print_deleted("dependency", &id)
        }
        other => bail!("unknown dependencies action '{}'", other),
    }
}

fn handle_google(conn: &rusqlite::Connection, action: &str, args: Vec<String>) -> Result<()> {
    match action {
        "sync" => {
            let flags = parse_flags(args)?;
            let calendar_id_filter = optional_string(&flags, "calendar-id");
            if flags.len() > usize::from(calendar_id_filter.is_some()) {
                bail!("google sync supports only --calendar-id");
            }

            let client = google::auth::GoogleClient::from_keyring()
                .ok_or_else(|| anyhow!("google credentials are not configured in keyring"))?;

            let mut calendars: Vec<_> = db::load_calendars(conn)?
                .into_iter()
                .filter(|cal| cal.source == models::CalendarSource::Google)
                .collect();

            if let Some(calendar_id) = calendar_id_filter.as_deref() {
                calendars.retain(|cal| cal.id == calendar_id);
                if calendars.is_empty() {
                    bail!(
                        "google calendar '{}' not found or not google-sourced",
                        calendar_id
                    );
                }
            }

            if calendars.is_empty() {
                bail!("no google calendars found to sync");
            }

            let rt = tokio::runtime::Runtime::new().context("failed to start tokio runtime")?;
            let mut summaries = Vec::with_capacity(calendars.len());
            let mut total_added = 0usize;
            let mut total_updated = 0usize;
            for calendar in &calendars {
                let (added, updated) = rt
                    .block_on(google::sync::sync_calendar(&client, calendar))
                    .with_context(|| format!("sync failed for calendar '{}'", calendar.name))?;
                total_added += added;
                total_updated += updated;
                summaries.push(serde_json::json!({
                    "calendar_id": calendar.id,
                    "calendar_name": calendar.name,
                    "google_id": calendar.google_id,
                    "events_added": added,
                    "events_updated": updated
                }));
            }

            print_json(&serde_json::json!({
                "status": "ok",
                "resource": "google-sync",
                "calendars_synced": summaries.len(),
                "events_added": total_added,
                "events_updated": total_updated,
                "results": summaries
            }))
        }
        other => bail!("unknown google action '{}'; supported action: sync", other),
    }
}

fn parse_flags(args: Vec<String>) -> Result<HashMap<String, Option<String>>> {
    let mut flags = HashMap::new();
    let mut iter = args.into_iter().peekable();
    while let Some(arg) = iter.next() {
        if !arg.starts_with("--") {
            bail!("unexpected positional argument '{}'", arg);
        }
        let key = arg.trim_start_matches("--").to_string();
        let value = match iter.peek() {
            Some(next) if !next.starts_with("--") => iter.next(),
            _ => None,
        };
        flags.insert(key, value);
    }
    Ok(flags)
}

fn expect_single_id(args: Vec<String>, resource_name: &str) -> Result<String> {
    if args.len() != 1 {
        bail!("expected exactly one {} id argument", resource_name);
    }
    Ok(args[0].clone())
}

fn split_id_and_flags(
    mut args: Vec<String>,
    resource_name: &str,
) -> Result<(String, HashMap<String, Option<String>>)> {
    if args.is_empty() {
        bail!("expected {} id argument before flags", resource_name);
    }
    let id = args.remove(0);
    Ok((id, parse_flags(args)?))
}

fn required_string(flags: &HashMap<String, Option<String>>, key: &str) -> Result<String> {
    flags
        .get(key)
        .and_then(|value| value.clone())
        .ok_or_else(|| anyhow!("missing required --{}", key))
}

fn optional_string(flags: &HashMap<String, Option<String>>, key: &str) -> Option<String> {
    flags.get(key).cloned().flatten()
}

fn string_or_default(flags: &HashMap<String, Option<String>>, key: &str, default: &str) -> String {
    optional_string(flags, key).unwrap_or_else(|| default.to_string())
}

fn parse_bool_flag(flags: &HashMap<String, Option<String>>, key: &str) -> Result<Option<bool>> {
    match flags.get(key) {
        None => Ok(None),
        Some(None) => Ok(Some(true)),
        Some(Some(value)) => match value.as_str() {
            "true" | "1" | "yes" => Ok(Some(true)),
            "false" | "0" | "no" => Ok(Some(false)),
            _ => bail!("invalid boolean for --{}: '{}'", key, value),
        },
    }
}

fn parse_i64_flag(flags: &HashMap<String, Option<String>>, key: &str) -> Result<Option<i64>> {
    match flags.get(key) {
        None => Ok(None),
        Some(None) => bail!("--{} requires a value", key),
        Some(Some(value)) => value
            .parse::<i64>()
            .map(Some)
            .with_context(|| format!("invalid integer for --{}: '{}'", key, value)),
    }
}

fn parse_calendar_source(value: &str) -> Result<models::CalendarSource> {
    match value {
        "local" => Ok(models::CalendarSource::Local),
        "google" => Ok(models::CalendarSource::Google),
        _ => bail!(
            "invalid calendar source '{}'; expected local or google",
            value
        ),
    }
}

fn parse_dependency_type(value: &str) -> Result<models::DependencyType> {
    match value {
        "blocks" => Ok(models::DependencyType::Blocks),
        "related" => Ok(models::DependencyType::Related),
        _ => bail!(
            "invalid dependency type '{}'; expected blocks or related",
            value
        ),
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_deleted(resource: &str, id: &str) -> Result<()> {
    print_json(&serde_json::json!({
        "status": "deleted",
        "resource": resource,
        "id": id,
    }))
}

fn require_resource<T>(resource: Option<T>, resource_name: &str, id: &str) -> Result<T> {
    resource.ok_or_else(|| anyhow!("{} '{}' not found", resource_name, id))
}

fn now_timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn validate_event_datetime(start_at: &str, end_at: &str) -> Result<()> {
    let start = chrono::NaiveDateTime::parse_from_str(start_at, "%Y-%m-%d %H:%M:%S").with_context(
        || {
            format!(
                "invalid start_at '{}'; expected YYYY-MM-DD HH:MM:SS",
                start_at
            )
        },
    )?;
    let end = chrono::NaiveDateTime::parse_from_str(end_at, "%Y-%m-%d %H:%M:%S")
        .with_context(|| format!("invalid end_at '{}'; expected YYYY-MM-DD HH:MM:SS", end_at))?;

    if end < start {
        bail!("end_at must be greater than or equal to start_at");
    }

    Ok(())
}

fn validate_dependency(
    conn: &rusqlite::Connection,
    from_event_id: &str,
    to_event_id: &str,
) -> Result<()> {
    if from_event_id == to_event_id {
        bail!("dependency endpoints must reference two distinct events");
    }

    require_resource(db::get_event(conn, from_event_id)?, "event", from_event_id)
        .context("from-event-id must reference an existing non-deleted event")?;
    require_resource(db::get_event(conn, to_event_id)?, "event", to_event_id)
        .context("to-event-id must reference an existing non-deleted event")?;
    Ok(())
}

fn print_help() {
    println!(
        concat!(
            "solverforge-calendar-cli - JSON-first CRUD companion for OpenClaw\n\n",
            "Usage:\n",
            "  solverforge-calendar-cli <resource> <action> [args...]\n\n",
            "Resources and actions:\n",
            "  calendars    list | get <id> | create [flags] | update <id> [flags] | delete <id>\n",
            "  projects     list | get <id> | create [flags] | update <id> [flags] | delete <id>\n",
            "  events       list [--from ... --to ...] | get <id> | create [flags] | update <id> [flags] | delete <id>\n",
            "  dependencies list | get <id> | create [flags] | update <id> [flags] | delete <id>\n",
            "  google       sync [--calendar-id <id>]\n\n",
            "Examples:\n",
            "  solverforge-calendar-cli calendars list\n",
            "  solverforge-calendar-cli calendars create --name Work --color '#50f872'\n",
            "  solverforge-calendar-cli events create --calendar-id <id> --title 'Standup' --start-at '2026-03-22 09:00:00' --end-at '2026-03-22 09:15:00'\n",
            "  solverforge-calendar-cli google sync\n"
        )
    );
}
