use anyhow::Context;
use chrono::NaiveDateTime;
use clap::{Args, Parser, Subcommand, ValueEnum};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{dag::EventDag, db, google, models};

#[derive(Debug, Parser)]
#[command(name = "solverforge-calendar-cli", about = "JSON-first automation CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Calendars {
        #[command(subcommand)]
        action: CalendarCommand,
    },
    Projects {
        #[command(subcommand)]
        action: ProjectCommand,
    },
    Events {
        #[command(subcommand)]
        action: EventCommand,
    },
    Dependencies {
        #[command(subcommand)]
        action: DependencyCommand,
    },
    Google {
        #[command(subcommand)]
        action: GoogleCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum CalendarCommand {
    List,
    Get { id: String },
    Create(CalendarCreateArgs),
    Update(CalendarUpdateArgs),
    Delete(CalendarDeleteArgs),
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    List,
    Get { id: String },
    Create(ProjectCreateArgs),
    Update(ProjectUpdateArgs),
    Delete(ProjectDeleteArgs),
}

#[derive(Debug, Subcommand)]
pub enum EventCommand {
    List(EventListArgs),
    Get { id: String },
    Create(EventCreateArgs),
    Update(EventUpdateArgs),
    Delete { id: String },
}

#[derive(Debug, Subcommand)]
pub enum DependencyCommand {
    List,
    Get { id: String },
    Create(DependencyCreateArgs),
    Update(DependencyUpdateArgs),
    Delete { id: String },
}

#[derive(Debug, Subcommand)]
pub enum GoogleCommand {
    Sync(GoogleSyncArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CalendarSourceArg {
    Local,
    Google,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum DependencyTypeArg {
    Blocks,
    Related,
}

#[derive(Debug, Args)]
pub struct CalendarCreateArgs {
    #[arg(long)]
    name: String,
    #[arg(long)]
    color: String,
    #[arg(long, value_enum, default_value_t = CalendarSourceArg::Local)]
    source: CalendarSourceArg,
    #[arg(long)]
    google_id: Option<String>,
    #[arg(long, default_value_t = true, value_parser = clap::builder::BoolishValueParser::new())]
    visible: bool,
    #[arg(long, default_value_t = 0)]
    position: i64,
}

#[derive(Debug, Args)]
pub struct CalendarUpdateArgs {
    id: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    color: Option<String>,
    #[arg(long, value_enum)]
    source: Option<CalendarSourceArg>,
    #[arg(long)]
    google_id: Option<String>,
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
    visible: Option<bool>,
    #[arg(long)]
    position: Option<i64>,
}

#[derive(Debug, Args)]
pub struct CalendarDeleteArgs {
    id: String,
    #[arg(long)]
    cascade_events: bool,
}

#[derive(Debug, Args)]
pub struct ProjectCreateArgs {
    #[arg(long)]
    name: String,
    #[arg(long)]
    color: String,
    #[arg(long)]
    description: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectUpdateArgs {
    id: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    color: Option<String>,
    #[arg(long)]
    description: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectDeleteArgs {
    id: String,
    #[arg(long)]
    detach_events: bool,
}

#[derive(Debug, Args)]
pub struct EventListArgs {
    #[arg(long, requires = "to", value_parser = parse_timestamp_arg)]
    from: Option<String>,
    #[arg(long, requires = "from", value_parser = parse_timestamp_arg)]
    to: Option<String>,
}

#[derive(Debug, Args)]
pub struct EventCreateArgs {
    #[arg(long)]
    calendar_id: String,
    #[arg(long)]
    title: String,
    #[arg(long)]
    project_id: Option<String>,
    #[arg(long)]
    description: Option<String>,
    #[arg(long)]
    location: Option<String>,
    #[arg(long, value_parser = parse_timestamp_arg)]
    start_at: String,
    #[arg(long, value_parser = parse_timestamp_arg)]
    end_at: String,
    #[arg(long, default_value_t = false, value_parser = clap::builder::BoolishValueParser::new())]
    all_day: bool,
    #[arg(long)]
    rrule: Option<String>,
    #[arg(long)]
    reminder_minutes: Option<i64>,
    #[arg(long, default_value = "UTC")]
    timezone: String,
}

#[derive(Debug, Args)]
pub struct EventUpdateArgs {
    id: String,
    #[arg(long)]
    calendar_id: Option<String>,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    project_id: Option<String>,
    #[arg(long)]
    clear_project_id: bool,
    #[arg(long)]
    description: Option<String>,
    #[arg(long)]
    clear_description: bool,
    #[arg(long)]
    location: Option<String>,
    #[arg(long)]
    clear_location: bool,
    #[arg(long, value_parser = parse_timestamp_arg)]
    start_at: Option<String>,
    #[arg(long, value_parser = parse_timestamp_arg)]
    end_at: Option<String>,
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
    all_day: Option<bool>,
    #[arg(long)]
    rrule: Option<String>,
    #[arg(long)]
    clear_rrule: bool,
    #[arg(long)]
    reminder_minutes: Option<i64>,
    #[arg(long)]
    clear_reminder_minutes: bool,
    #[arg(long)]
    timezone: Option<String>,
}

#[derive(Debug, Args)]
pub struct DependencyCreateArgs {
    #[arg(long)]
    from_event_id: String,
    #[arg(long)]
    to_event_id: String,
    #[arg(long, value_enum, default_value_t = DependencyTypeArg::Blocks)]
    dependency_type: DependencyTypeArg,
}

#[derive(Debug, Args)]
pub struct DependencyUpdateArgs {
    id: String,
    #[arg(long)]
    from_event_id: Option<String>,
    #[arg(long)]
    to_event_id: Option<String>,
    #[arg(long, value_enum)]
    dependency_type: Option<DependencyTypeArg>,
}

#[derive(Debug, Args)]
pub struct GoogleSyncArgs {
    #[arg(long)]
    calendar_id: Option<String>,
}

#[derive(Debug)]
pub struct CliError {
    pub code: &'static str,
    pub message: String,
}

impl CliError {
    fn validation(message: impl Into<String>) -> Self {
        Self {
            code: "validation_error",
            message: message.into(),
        }
    }

    fn not_found(resource: &'static str, id: &str) -> Self {
        Self {
            code: "not_found",
            message: format!("{} '{}' not found", resource, id),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            code: "conflict",
            message: message.into(),
        }
    }

    fn external(message: impl Into<String>) -> Self {
        Self {
            code: "external_error",
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal_error",
            message: message.into(),
        }
    }

    pub fn invalid_arguments(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_arguments",
            message: message.into(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorPayload<'a> {
    status: &'static str,
    code: &'a str,
    message: &'a str,
}

#[derive(Debug, Serialize)]
struct SuccessPayload<T> {
    status: &'static str,
    data: T,
}

#[derive(Debug, Serialize)]
struct DeleteData<'a> {
    resource: &'a str,
    id: String,
}

#[derive(Debug, Serialize)]
struct SyncCalendarData {
    calendar_id: String,
    calendar_name: String,
    google_id: Option<String>,
    events_added: usize,
    events_updated: usize,
}

pub fn error_value(err: &CliError) -> Value {
    serde_json::to_value(ErrorPayload {
        status: "error",
        code: err.code,
        message: &err.message,
    })
    .expect("serializable error payload")
}

pub fn execute(cli: Cli) -> Result<Value, CliError> {
    let conn = db::open().map_err(|e| CliError::internal(e.to_string()))?;
    execute_with_connection(&conn, cli)
}

pub fn execute_with_connection(conn: &Connection, cli: Cli) -> Result<Value, CliError> {
    let data = match cli.command {
        Command::Calendars { action } => handle_calendars(conn, action)?,
        Command::Projects { action } => handle_projects(conn, action)?,
        Command::Events { action } => handle_events(conn, action)?,
        Command::Dependencies { action } => handle_dependencies(conn, action)?,
        Command::Google { action } => handle_google(conn, action)?,
    };
    Ok(success_value(data))
}

fn success_value<T: Serialize>(data: T) -> Value {
    serde_json::to_value(SuccessPayload { status: "ok", data }).expect("serializable success")
}

fn parse_timestamp_arg(value: &str) -> Result<String, String> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .map_err(|_| {
            format!(
                "invalid timestamp '{}'; expected YYYY-MM-DD HH:MM:SS",
                value
            )
        })
}

fn handle_calendars(conn: &Connection, action: CalendarCommand) -> Result<Value, CliError> {
    match action {
        CalendarCommand::List => Ok(json!(db::load_calendars(conn).map_err(internal_error)?)),
        CalendarCommand::Get { id } => Ok(json!(require_resource(
            db::get_calendar(conn, &id).map_err(internal_error)?,
            "calendar",
            &id
        )?)),
        CalendarCommand::Create(args) => {
            let now = timestamp_now();
            let name = non_empty(args.name, "name")?;
            let color = non_empty(args.color, "color")?;
            let source = calendar_source_from_arg(args.source);
            let google_id = normalize_optional(args.google_id);
            validate_calendar_source(&source, google_id.as_deref())?;

            let calendar = models::Calendar {
                id: Uuid::new_v4().to_string(),
                name,
                color,
                source,
                google_id,
                visible: args.visible,
                position: args.position,
                created_at: now.clone(),
                updated_at: now,
                deleted_at: None,
            };
            db::insert_calendar(conn, &calendar).map_err(internal_error)?;
            Ok(json!(calendar))
        }
        CalendarCommand::Update(args) => {
            let mut calendar = require_resource(
                db::get_calendar(conn, &args.id).map_err(internal_error)?,
                "calendar",
                &args.id,
            )?;
            if let Some(name) = args.name {
                calendar.name = non_empty(name, "name")?;
            }
            if let Some(color) = args.color {
                calendar.color = non_empty(color, "color")?;
            }
            if let Some(source) = args.source {
                calendar.source = calendar_source_from_arg(source);
            }
            if let Some(visible) = args.visible {
                calendar.visible = visible;
            }
            if let Some(position) = args.position {
                calendar.position = position;
            }
            if args.google_id.is_some() {
                calendar.google_id = normalize_optional(args.google_id);
            }
            if calendar.source == models::CalendarSource::Local {
                calendar.google_id = None;
            }
            validate_calendar_source(&calendar.source, calendar.google_id.as_deref())?;
            db::update_calendar(conn, &calendar).map_err(internal_error)?;
            Ok(json!(require_resource(
                db::get_calendar(conn, &args.id).map_err(internal_error)?,
                "calendar",
                &args.id,
            )?))
        }
        CalendarCommand::Delete(args) => {
            require_resource(
                db::get_calendar(conn, &args.id).map_err(internal_error)?,
                "calendar",
                &args.id,
            )?;
            let active_events =
                db::count_active_events_for_calendar(conn, &args.id).map_err(internal_error)?;
            if active_events > 0 && !args.cascade_events {
                return Err(CliError::conflict(
                    "calendar has active events; rerun with --cascade-events to delete them too",
                ));
            }

            let tx = conn.unchecked_transaction().map_err(internal_error)?;
            if args.cascade_events {
                for event_id in
                    db::load_active_event_ids_for_calendar(&tx, &args.id).map_err(internal_error)?
                {
                    db::soft_delete_event(&tx, &event_id).map_err(internal_error)?;
                }
            }
            db::soft_delete_calendar(&tx, &args.id).map_err(internal_error)?;
            db::delete_sync_token(&tx, &args.id).map_err(internal_error)?;
            tx.commit().map_err(internal_error)?;
            Ok(json!(DeleteData {
                resource: "calendar",
                id: args.id,
            }))
        }
    }
}

fn handle_projects(conn: &Connection, action: ProjectCommand) -> Result<Value, CliError> {
    match action {
        ProjectCommand::List => Ok(json!(db::load_projects(conn).map_err(internal_error)?)),
        ProjectCommand::Get { id } => Ok(json!(require_resource(
            db::get_project(conn, &id).map_err(internal_error)?,
            "project",
            &id
        )?)),
        ProjectCommand::Create(args) => {
            let now = timestamp_now();
            let project = models::Project {
                id: Uuid::new_v4().to_string(),
                name: non_empty(args.name, "name")?,
                color: non_empty(args.color, "color")?,
                description: normalize_optional(args.description),
                created_at: now.clone(),
                updated_at: now,
                deleted_at: None,
            };
            db::insert_project(conn, &project).map_err(internal_error)?;
            Ok(json!(project))
        }
        ProjectCommand::Update(args) => {
            let mut project = require_resource(
                db::get_project(conn, &args.id).map_err(internal_error)?,
                "project",
                &args.id,
            )?;
            if let Some(name) = args.name {
                project.name = non_empty(name, "name")?;
            }
            if let Some(color) = args.color {
                project.color = non_empty(color, "color")?;
            }
            if args.description.is_some() {
                project.description = normalize_optional(args.description);
            }
            db::update_project(conn, &project).map_err(internal_error)?;
            Ok(json!(require_resource(
                db::get_project(conn, &args.id).map_err(internal_error)?,
                "project",
                &args.id,
            )?))
        }
        ProjectCommand::Delete(args) => {
            require_resource(
                db::get_project(conn, &args.id).map_err(internal_error)?,
                "project",
                &args.id,
            )?;
            let active_events =
                db::count_active_events_for_project(conn, &args.id).map_err(internal_error)?;
            if active_events > 0 && !args.detach_events {
                return Err(CliError::conflict(
                    "project has active events; rerun with --detach-events to clear project_id first",
                ));
            }

            let tx = conn.unchecked_transaction().map_err(internal_error)?;
            if args.detach_events {
                db::clear_project_id_for_project(&tx, &args.id).map_err(internal_error)?;
            }
            db::soft_delete_project(&tx, &args.id).map_err(internal_error)?;
            tx.commit().map_err(internal_error)?;
            Ok(json!(DeleteData {
                resource: "project",
                id: args.id,
            }))
        }
    }
}

fn handle_events(conn: &Connection, action: EventCommand) -> Result<Value, CliError> {
    match action {
        EventCommand::List(args) => {
            let events = match (args.from, args.to) {
                (Some(from), Some(to)) => {
                    db::load_events_in_range(conn, &from, &to).map_err(internal_error)?
                }
                (None, None) => db::load_events(conn).map_err(internal_error)?,
                _ => unreachable!("clap enforces paired args"),
            };
            Ok(json!(events))
        }
        EventCommand::Get { id } => Ok(json!(require_resource(
            db::get_event(conn, &id).map_err(internal_error)?,
            "event",
            &id
        )?)),
        EventCommand::Create(args) => {
            ensure_calendar_exists(conn, &args.calendar_id)?;
            if let Some(project_id) = args.project_id.as_deref() {
                ensure_project_exists(conn, project_id)?;
            }
            ensure_title(&args.title)?;
            validate_event_datetime(&args.start_at, &args.end_at)?;
            let now = timestamp_now();
            let event = models::Event {
                id: Uuid::new_v4().to_string(),
                calendar_id: args.calendar_id,
                project_id: normalize_optional(args.project_id),
                title: args.title.trim().to_string(),
                description: normalize_optional(args.description),
                location: normalize_optional(args.location),
                start_at: args.start_at,
                end_at: args.end_at,
                all_day: args.all_day,
                rrule: normalize_optional(args.rrule),
                google_id: None,
                google_etag: None,
                reminder_minutes: args.reminder_minutes,
                timezone: non_empty(args.timezone, "timezone")?,
                created_at: now.clone(),
                updated_at: now,
                deleted_at: None,
            };
            db::insert_event(conn, &event).map_err(internal_error)?;
            Ok(json!(event))
        }
        EventCommand::Update(args) => {
            let mut event = require_resource(
                db::get_event(conn, &args.id).map_err(internal_error)?,
                "event",
                &args.id,
            )?;

            if let Some(calendar_id) = args.calendar_id {
                ensure_calendar_exists(conn, &calendar_id)?;
                event.calendar_id = calendar_id;
            }
            if let Some(title) = args.title {
                ensure_title(&title)?;
                event.title = title.trim().to_string();
            }
            if args.clear_project_id {
                event.project_id = None;
            } else if let Some(project_id) = args.project_id {
                ensure_project_exists(conn, &project_id)?;
                event.project_id = Some(project_id);
            }
            if args.clear_description {
                event.description = None;
            } else if args.description.is_some() {
                event.description = normalize_optional(args.description);
            }
            if args.clear_location {
                event.location = None;
            } else if args.location.is_some() {
                event.location = normalize_optional(args.location);
            }
            if let Some(start_at) = args.start_at {
                event.start_at = start_at;
            }
            if let Some(end_at) = args.end_at {
                event.end_at = end_at;
            }
            if let Some(all_day) = args.all_day {
                event.all_day = all_day;
            }
            if args.clear_rrule {
                event.rrule = None;
            } else if args.rrule.is_some() {
                event.rrule = normalize_optional(args.rrule);
            }
            if args.clear_reminder_minutes {
                event.reminder_minutes = None;
            } else if let Some(reminder_minutes) = args.reminder_minutes {
                event.reminder_minutes = Some(reminder_minutes);
            }
            if let Some(timezone) = args.timezone {
                event.timezone = non_empty(timezone, "timezone")?;
            }

            validate_event_datetime(&event.start_at, &event.end_at)?;
            db::update_event(conn, &event).map_err(internal_error)?;
            Ok(json!(require_resource(
                db::get_event(conn, &args.id).map_err(internal_error)?,
                "event",
                &args.id,
            )?))
        }
        EventCommand::Delete { id } => {
            require_resource(
                db::get_event(conn, &id).map_err(internal_error)?,
                "event",
                &id,
            )?;
            db::soft_delete_event(conn, &id).map_err(internal_error)?;
            Ok(json!(DeleteData {
                resource: "event",
                id,
            }))
        }
    }
}

fn handle_dependencies(conn: &Connection, action: DependencyCommand) -> Result<Value, CliError> {
    match action {
        DependencyCommand::List => Ok(json!(db::load_dependencies(conn).map_err(internal_error)?)),
        DependencyCommand::Get { id } => Ok(json!(require_resource(
            db::get_dependency(conn, &id).map_err(internal_error)?,
            "dependency",
            &id,
        )?)),
        DependencyCommand::Create(args) => {
            validate_dependency_endpoints(conn, &args.from_event_id, &args.to_event_id)?;
            validate_dependency_edge(
                conn,
                &args.from_event_id,
                &args.to_event_id,
                dependency_type_from_arg(args.dependency_type),
                None,
            )?;
            let now = timestamp_now();
            let dependency = models::EventDependency {
                id: Uuid::new_v4().to_string(),
                from_event_id: args.from_event_id,
                to_event_id: args.to_event_id,
                dependency_type: dependency_type_from_arg(args.dependency_type),
                created_at: now.clone(),
                updated_at: now,
            };
            db::insert_dependency(conn, &dependency).map_err(internal_error)?;
            Ok(json!(dependency))
        }
        DependencyCommand::Update(args) => {
            let mut dependency = require_resource(
                db::get_dependency(conn, &args.id).map_err(internal_error)?,
                "dependency",
                &args.id,
            )?;
            if let Some(from_event_id) = args.from_event_id {
                dependency.from_event_id = from_event_id;
            }
            if let Some(to_event_id) = args.to_event_id {
                dependency.to_event_id = to_event_id;
            }
            if let Some(dependency_type) = args.dependency_type {
                dependency.dependency_type = dependency_type_from_arg(dependency_type);
            }
            validate_dependency_endpoints(
                conn,
                &dependency.from_event_id,
                &dependency.to_event_id,
            )?;
            validate_dependency_edge(
                conn,
                &dependency.from_event_id,
                &dependency.to_event_id,
                dependency.dependency_type.clone(),
                Some(dependency.id.as_str()),
            )?;
            db::update_dependency(conn, &dependency).map_err(internal_error)?;
            Ok(json!(require_resource(
                db::get_dependency(conn, &args.id).map_err(internal_error)?,
                "dependency",
                &args.id,
            )?))
        }
        DependencyCommand::Delete { id } => {
            require_resource(
                db::get_dependency(conn, &id).map_err(internal_error)?,
                "dependency",
                &id,
            )?;
            db::delete_dependency(conn, &id).map_err(internal_error)?;
            Ok(json!(DeleteData {
                resource: "dependency",
                id,
            }))
        }
    }
}

fn handle_google(conn: &Connection, action: GoogleCommand) -> Result<Value, CliError> {
    match action {
        GoogleCommand::Sync(args) => {
            let client = google::auth::GoogleClient::from_keyring().ok_or_else(|| {
                CliError::external("google credentials are not configured in keyring")
            })?;

            let mut calendars: Vec<_> = db::load_calendars(conn)
                .map_err(internal_error)?
                .into_iter()
                .filter(|calendar| calendar.source == models::CalendarSource::Google)
                .collect();

            if let Some(calendar_id) = args.calendar_id.as_deref() {
                calendars.retain(|calendar| calendar.id == calendar_id);
                if calendars.is_empty() {
                    return Err(CliError::not_found("google calendar", calendar_id));
                }
            }

            if calendars.is_empty() {
                return Err(CliError::conflict("no google calendars found to sync"));
            }

            let rt = tokio::runtime::Runtime::new()
                .context("failed to start tokio runtime")
                .map_err(|e| CliError::internal(e.to_string()))?;
            let mut results = Vec::with_capacity(calendars.len());
            let mut total_added = 0usize;
            let mut total_updated = 0usize;

            for calendar in &calendars {
                let sync_token = db::get_sync_token(conn, &calendar.id).map_err(internal_error)?;
                let delta = rt
                    .block_on(google::sync::fetch_calendar_delta(
                        &client,
                        calendar,
                        sync_token.as_deref(),
                    ))
                    .with_context(|| format!("sync failed for calendar '{}'", calendar.name))
                    .map_err(|e| CliError::external(e.to_string()))?;
                let (added, updated) = google::sync::apply_calendar_sync(conn, calendar, delta)
                    .with_context(|| {
                        format!(
                            "failed to persist sync results for calendar '{}'",
                            calendar.name
                        )
                    })
                    .map_err(|e| CliError::external(e.to_string()))?;
                total_added += added;
                total_updated += updated;
                results.push(SyncCalendarData {
                    calendar_id: calendar.id.clone(),
                    calendar_name: calendar.name.clone(),
                    google_id: calendar.google_id.clone(),
                    events_added: added,
                    events_updated: updated,
                });
            }

            Ok(json!({
                "calendars_synced": results.len(),
                "events_added": total_added,
                "events_updated": total_updated,
                "results": results,
            }))
        }
    }
}

fn timestamp_now() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn internal_error(err: impl std::fmt::Display) -> CliError {
    CliError::internal(err.to_string())
}

fn require_resource<T>(resource: Option<T>, name: &'static str, id: &str) -> Result<T, CliError> {
    resource.ok_or_else(|| CliError::not_found(name, id))
}

fn non_empty(value: String, field: &'static str) -> Result<String, CliError> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(CliError::validation(format!("{} cannot be empty", field)));
    }
    Ok(trimmed)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn ensure_title(title: &str) -> Result<(), CliError> {
    if title.trim().is_empty() {
        return Err(CliError::validation("title cannot be empty"));
    }
    Ok(())
}

fn validate_event_datetime(start_at: &str, end_at: &str) -> Result<(), CliError> {
    let start = NaiveDateTime::parse_from_str(start_at, "%Y-%m-%d %H:%M:%S")
        .map_err(|_| CliError::validation("invalid start_at timestamp"))?;
    let end = NaiveDateTime::parse_from_str(end_at, "%Y-%m-%d %H:%M:%S")
        .map_err(|_| CliError::validation("invalid end_at timestamp"))?;
    if end < start {
        return Err(CliError::validation(
            "end_at must be greater than or equal to start_at",
        ));
    }
    Ok(())
}

fn validate_calendar_source(
    source: &models::CalendarSource,
    google_id: Option<&str>,
) -> Result<(), CliError> {
    match source {
        models::CalendarSource::Google if google_id.is_none() => {
            Err(CliError::validation("google calendars require --google-id"))
        }
        models::CalendarSource::Local if google_id.is_some() => Err(CliError::validation(
            "local calendars cannot set --google-id",
        )),
        _ => Ok(()),
    }
}

fn ensure_calendar_exists(conn: &Connection, calendar_id: &str) -> Result<(), CliError> {
    require_resource(
        db::get_calendar(conn, calendar_id).map_err(internal_error)?,
        "calendar",
        calendar_id,
    )?;
    Ok(())
}

fn ensure_project_exists(conn: &Connection, project_id: &str) -> Result<(), CliError> {
    require_resource(
        db::get_project(conn, project_id).map_err(internal_error)?,
        "project",
        project_id,
    )?;
    Ok(())
}

fn ensure_event_exists(conn: &Connection, event_id: &str) -> Result<(), CliError> {
    require_resource(
        db::get_event(conn, event_id).map_err(internal_error)?,
        "event",
        event_id,
    )?;
    Ok(())
}

fn validate_dependency_endpoints(
    conn: &Connection,
    from_event_id: &str,
    to_event_id: &str,
) -> Result<(), CliError> {
    if from_event_id == to_event_id {
        return Err(CliError::validation(
            "dependency endpoints must reference two distinct events",
        ));
    }
    ensure_event_exists(conn, from_event_id)?;
    ensure_event_exists(conn, to_event_id)?;
    Ok(())
}

fn validate_dependency_edge(
    conn: &Connection,
    from_event_id: &str,
    to_event_id: &str,
    dependency_type: models::DependencyType,
    exclude_dependency_id: Option<&str>,
) -> Result<(), CliError> {
    let dependencies = db::load_dependencies(conn).map_err(internal_error)?;
    if dependencies.iter().any(|dependency| {
        Some(dependency.id.as_str()) != exclude_dependency_id
            && dependency.from_event_id == from_event_id
            && dependency.to_event_id == to_event_id
    }) {
        return Err(CliError::conflict("dependency edge already exists"));
    }

    if dependency_type != models::DependencyType::Blocks {
        return Ok(());
    }

    let active_blocks: Vec<_> = dependencies
        .into_iter()
        .filter(|dependency| {
            dependency.dependency_type == models::DependencyType::Blocks
                && Some(dependency.id.as_str()) != exclude_dependency_id
        })
        .collect();
    let mut dag = EventDag::from_dependencies(&active_blocks);
    if dag.add_edge(from_event_id, to_event_id).is_err() {
        return Err(CliError::conflict(
            "dependency would create a cycle in blocks edges",
        ));
    }
    Ok(())
}

fn calendar_source_from_arg(source: CalendarSourceArg) -> models::CalendarSource {
    match source {
        CalendarSourceArg::Local => models::CalendarSource::Local,
        CalendarSourceArg::Google => models::CalendarSource::Google,
    }
}

fn dependency_type_from_arg(dependency_type: DependencyTypeArg) -> models::DependencyType {
    match dependency_type {
        DependencyTypeArg::Blocks => models::DependencyType::Blocks,
        DependencyTypeArg::Related => models::DependencyType::Related,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_test_db() -> (TempDir, Connection) {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("calendar.db");
        let conn = db::open_at(&db_path).unwrap();
        (temp, conn)
    }

    fn seed_calendar(conn: &Connection, name: &str) -> models::Calendar {
        let now = timestamp_now();
        let calendar = models::Calendar {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            color: "#82FB9C".to_string(),
            source: models::CalendarSource::Local,
            google_id: None,
            visible: true,
            position: 0,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        db::insert_calendar(conn, &calendar).unwrap();
        calendar
    }

    fn seed_project(conn: &Connection, name: &str) -> models::Project {
        let now = timestamp_now();
        let project = models::Project {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            color: "#ffaa00".to_string(),
            description: None,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        db::insert_project(conn, &project).unwrap();
        project
    }

    fn seed_event(
        conn: &Connection,
        calendar_id: &str,
        project_id: Option<String>,
        title: &str,
    ) -> models::Event {
        let now = timestamp_now();
        let event = models::Event {
            id: Uuid::new_v4().to_string(),
            calendar_id: calendar_id.to_string(),
            project_id,
            title: title.to_string(),
            description: None,
            location: None,
            start_at: "2026-03-30 09:00:00".to_string(),
            end_at: "2026-03-30 10:00:00".to_string(),
            all_day: false,
            rrule: None,
            google_id: None,
            google_etag: None,
            reminder_minutes: None,
            timezone: "UTC".to_string(),
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        db::insert_event(conn, &event).unwrap();
        event
    }

    #[test]
    fn blocks_cycle_is_rejected() {
        let (_temp, conn) = open_test_db();
        let calendar = seed_calendar(&conn, "Work");
        let event_a = seed_event(&conn, &calendar.id, None, "A");
        let event_b = seed_event(&conn, &calendar.id, None, "B");

        let first = Cli {
            command: Command::Dependencies {
                action: DependencyCommand::Create(DependencyCreateArgs {
                    from_event_id: event_a.id.clone(),
                    to_event_id: event_b.id.clone(),
                    dependency_type: DependencyTypeArg::Blocks,
                }),
            },
        };
        execute_with_connection(&conn, first).unwrap();

        let second = Cli {
            command: Command::Dependencies {
                action: DependencyCommand::Create(DependencyCreateArgs {
                    from_event_id: event_b.id.clone(),
                    to_event_id: event_a.id.clone(),
                    dependency_type: DependencyTypeArg::Blocks,
                }),
            },
        };
        let err = execute_with_connection(&conn, second).unwrap_err();
        assert_eq!(err.code, "conflict");
    }

    #[test]
    fn project_delete_requires_detach_flag() {
        let (_temp, conn) = open_test_db();
        let calendar = seed_calendar(&conn, "Work");
        let project = seed_project(&conn, "Launch");
        seed_event(&conn, &calendar.id, Some(project.id.clone()), "Milestone");

        let cli = Cli {
            command: Command::Projects {
                action: ProjectCommand::Delete(ProjectDeleteArgs {
                    id: project.id.clone(),
                    detach_events: false,
                }),
            },
        };
        let err = execute_with_connection(&conn, cli).unwrap_err();
        assert_eq!(err.code, "conflict");
    }

    #[test]
    fn project_delete_with_detach_clears_project_id() {
        let (_temp, conn) = open_test_db();
        let calendar = seed_calendar(&conn, "Work");
        let project = seed_project(&conn, "Launch");
        let event = seed_event(&conn, &calendar.id, Some(project.id.clone()), "Milestone");

        let cli = Cli {
            command: Command::Projects {
                action: ProjectCommand::Delete(ProjectDeleteArgs {
                    id: project.id.clone(),
                    detach_events: true,
                }),
            },
        };
        execute_with_connection(&conn, cli).unwrap();

        let updated = db::get_event(&conn, &event.id).unwrap().unwrap();
        assert_eq!(updated.project_id, None);
        assert!(db::get_project(&conn, &project.id).unwrap().is_none());
    }
}
