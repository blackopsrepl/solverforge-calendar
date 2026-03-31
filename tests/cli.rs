use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn cli_command(temp: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("solverforge-calendar-cli").unwrap();
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd
}

fn read_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

fn first_calendar_id(temp: &TempDir) -> String {
    let calendars = cli_command(temp)
        .args(["calendars", "list"])
        .output()
        .unwrap();
    assert!(calendars.status.success());
    let calendars_json = read_json(&calendars.stdout);
    calendars_json["data"][0]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn unknown_flag_returns_json_error() {
    let temp = TempDir::new().unwrap();
    let output = cli_command(&temp)
        .args(["events", "list", "--bogus"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let err = read_json(&output.stderr);
    assert_eq!(err["status"], "error");
    assert_eq!(err["code"], "invalid_arguments");
}

#[test]
fn agent_wrapper_script_exists() {
    assert!(Path::new("scripts/solverforge-calendar-cli").exists());
}

#[test]
fn event_crud_works_against_isolated_db() {
    let temp = TempDir::new().unwrap();
    let calendar_id = first_calendar_id(&temp);

    let created = cli_command(&temp)
        .args([
            "events",
            "create",
            "--calendar-id",
            &calendar_id,
            "--title",
            "Planning",
            "--start-at",
            "2026-03-30 09:00:00",
            "--end-at",
            "2026-03-30 10:00:00",
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    let created_json = read_json(&created.stdout);
    let event_id = created_json["data"]["id"].as_str().unwrap().to_string();

    let updated = cli_command(&temp)
        .args([
            "events",
            "update",
            &event_id,
            "--location",
            "HQ",
            "--reminder-minutes",
            "30",
        ])
        .output()
        .unwrap();
    assert!(updated.status.success());
    let updated_json = read_json(&updated.stdout);
    assert_eq!(updated_json["data"]["location"], "HQ");
    assert_eq!(updated_json["data"]["reminder_minutes"], 30);

    let listed = cli_command(&temp)
        .args(["events", "list"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed_json = read_json(&listed.stdout);
    assert_eq!(listed_json["data"].as_array().unwrap().len(), 1);
}

#[test]
fn calendar_crud_and_source_validation_work() {
    let temp = TempDir::new().unwrap();

    let invalid_local = cli_command(&temp)
        .args([
            "calendars",
            "create",
            "--name",
            "Local",
            "--color",
            "#123456",
            "--google-id",
            "google-local",
        ])
        .output()
        .unwrap();
    assert_eq!(invalid_local.status.code(), Some(1));
    let invalid_local_json = read_json(&invalid_local.stderr);
    assert_eq!(invalid_local_json["code"], "validation_error");

    let created = cli_command(&temp)
        .args([
            "calendars",
            "create",
            "--name",
            "Work",
            "--color",
            "#50f872",
            "--source",
            "google",
            "--google-id",
            "work@example.com",
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    let created_json = read_json(&created.stdout);
    let calendar_id = created_json["data"]["id"].as_str().unwrap().to_string();

    let fetched = cli_command(&temp)
        .args(["calendars", "get", &calendar_id])
        .output()
        .unwrap();
    assert!(fetched.status.success());
    let fetched_json = read_json(&fetched.stdout);
    assert_eq!(fetched_json["data"]["google_id"], "work@example.com");

    let updated = cli_command(&temp)
        .args([
            "calendars",
            "update",
            &calendar_id,
            "--source",
            "local",
            "--name",
            "Personal",
        ])
        .output()
        .unwrap();
    assert!(updated.status.success());
    let updated_json = read_json(&updated.stdout);
    assert_eq!(updated_json["data"]["source"], "local");
    assert!(updated_json["data"]["google_id"].is_null());
    assert_eq!(updated_json["data"]["name"], "Personal");
}

#[test]
fn project_crud_works() {
    let temp = TempDir::new().unwrap();

    let created = cli_command(&temp)
        .args([
            "projects",
            "create",
            "--name",
            "Launch",
            "--color",
            "#ffaa00",
            "--description",
            "Initial launch",
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    let created_json = read_json(&created.stdout);
    let project_id = created_json["data"]["id"].as_str().unwrap().to_string();

    let updated = cli_command(&temp)
        .args([
            "projects",
            "update",
            &project_id,
            "--description",
            "",
            "--name",
            "Launch v2",
        ])
        .output()
        .unwrap();
    assert!(updated.status.success());
    let updated_json = read_json(&updated.stdout);
    assert_eq!(updated_json["data"]["name"], "Launch v2");
    assert!(updated_json["data"]["description"].is_null());

    let listed = cli_command(&temp)
        .args(["projects", "list"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed_json = read_json(&listed.stdout);
    assert_eq!(listed_json["data"].as_array().unwrap().len(), 1);
}

#[test]
fn events_support_range_lists_and_clear_conflicts() {
    let temp = TempDir::new().unwrap();
    let calendar_id = first_calendar_id(&temp);

    let project = cli_command(&temp)
        .args([
            "projects", "create", "--name", "Launch", "--color", "#ffaa00",
        ])
        .output()
        .unwrap();
    assert!(project.status.success());
    let project_json = read_json(&project.stdout);
    let project_id = project_json["data"]["id"].as_str().unwrap().to_string();

    let first = cli_command(&temp)
        .args([
            "events",
            "create",
            "--calendar-id",
            &calendar_id,
            "--project-id",
            &project_id,
            "--title",
            "Planning",
            "--description",
            "Discuss plan",
            "--start-at",
            "2026-03-30 09:00:00",
            "--end-at",
            "2026-03-30 10:00:00",
        ])
        .output()
        .unwrap();
    assert!(first.status.success());
    let first_json = read_json(&first.stdout);
    let event_id = first_json["data"]["id"].as_str().unwrap().to_string();

    let second = cli_command(&temp)
        .args([
            "events",
            "create",
            "--calendar-id",
            &calendar_id,
            "--title",
            "Retro",
            "--start-at",
            "2026-03-31 09:00:00",
            "--end-at",
            "2026-03-31 10:00:00",
        ])
        .output()
        .unwrap();
    assert!(second.status.success());

    let ranged = cli_command(&temp)
        .args([
            "events",
            "list",
            "--from",
            "2026-03-30 00:00:00",
            "--to",
            "2026-03-30 23:59:59",
        ])
        .output()
        .unwrap();
    assert!(ranged.status.success());
    let ranged_json = read_json(&ranged.stdout);
    assert_eq!(ranged_json["data"].as_array().unwrap().len(), 1);

    let cleared = cli_command(&temp)
        .args([
            "events",
            "update",
            &event_id,
            "--clear-description",
            "--clear-project-id",
        ])
        .output()
        .unwrap();
    assert!(cleared.status.success());
    let cleared_json = read_json(&cleared.stdout);
    assert!(cleared_json["data"]["description"].is_null());
    assert!(cleared_json["data"]["project_id"].is_null());

    let conflicting = cli_command(&temp)
        .args([
            "events",
            "update",
            &event_id,
            "--description",
            "new",
            "--clear-description",
        ])
        .output()
        .unwrap();
    assert_eq!(conflicting.status.code(), Some(1));
    let conflicting_json = read_json(&conflicting.stderr);
    assert_eq!(conflicting_json["code"], "validation_error");
}

#[test]
fn dependency_crud_and_validation_work() {
    let temp = TempDir::new().unwrap();
    let calendar_id = first_calendar_id(&temp);

    let first = cli_command(&temp)
        .args([
            "events",
            "create",
            "--calendar-id",
            &calendar_id,
            "--title",
            "A",
            "--start-at",
            "2026-03-30 09:00:00",
            "--end-at",
            "2026-03-30 10:00:00",
        ])
        .output()
        .unwrap();
    let first_json = read_json(&first.stdout);
    let event_a = first_json["data"]["id"].as_str().unwrap().to_string();

    let second = cli_command(&temp)
        .args([
            "events",
            "create",
            "--calendar-id",
            &calendar_id,
            "--title",
            "B",
            "--start-at",
            "2026-03-30 11:00:00",
            "--end-at",
            "2026-03-30 12:00:00",
        ])
        .output()
        .unwrap();
    let second_json = read_json(&second.stdout);
    let event_b = second_json["data"]["id"].as_str().unwrap().to_string();

    let created = cli_command(&temp)
        .args([
            "dependencies",
            "create",
            "--from-event-id",
            &event_a,
            "--to-event-id",
            &event_b,
            "--dependency-type",
            "related",
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    let created_json = read_json(&created.stdout);
    let dependency_id = created_json["data"]["id"].as_str().unwrap().to_string();

    let duplicate = cli_command(&temp)
        .args([
            "dependencies",
            "create",
            "--from-event-id",
            &event_a,
            "--to-event-id",
            &event_b,
            "--dependency-type",
            "related",
        ])
        .output()
        .unwrap();
    assert_eq!(duplicate.status.code(), Some(1));
    let duplicate_json = read_json(&duplicate.stderr);
    assert_eq!(duplicate_json["code"], "conflict");

    let updated = cli_command(&temp)
        .args([
            "dependencies",
            "update",
            &dependency_id,
            "--dependency-type",
            "blocks",
        ])
        .output()
        .unwrap();
    assert!(updated.status.success());
    let updated_json = read_json(&updated.stdout);
    assert_eq!(updated_json["data"]["dependency_type"], "blocks");

    let listed = cli_command(&temp)
        .args(["dependencies", "list"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed_json = read_json(&listed.stdout);
    assert_eq!(listed_json["data"].as_array().unwrap().len(), 1);

    let deleted = cli_command(&temp)
        .args(["dependencies", "delete", &dependency_id])
        .output()
        .unwrap();
    assert!(deleted.status.success());

    let empty = cli_command(&temp)
        .args(["dependencies", "list"])
        .output()
        .unwrap();
    assert!(empty.status.success());
    let empty_json = read_json(&empty.stdout);
    assert!(empty_json["data"].as_array().unwrap().is_empty());
}

#[test]
fn calendar_delete_requires_explicit_cascade_flag() {
    let temp = TempDir::new().unwrap();

    let created_calendar = cli_command(&temp)
        .args([
            "calendars",
            "create",
            "--name",
            "Work",
            "--color",
            "#50f872",
        ])
        .output()
        .unwrap();
    assert!(created_calendar.status.success());
    let calendar_json = read_json(&created_calendar.stdout);
    let calendar_id = calendar_json["data"]["id"].as_str().unwrap().to_string();

    let created_event = cli_command(&temp)
        .args([
            "events",
            "create",
            "--calendar-id",
            &calendar_id,
            "--title",
            "Standup",
            "--start-at",
            "2026-03-30 09:00:00",
            "--end-at",
            "2026-03-30 09:30:00",
        ])
        .output()
        .unwrap();
    assert!(created_event.status.success());

    let blocked_delete = cli_command(&temp)
        .args(["calendars", "delete", &calendar_id])
        .output()
        .unwrap();
    assert_eq!(blocked_delete.status.code(), Some(1));
    let blocked_json = read_json(&blocked_delete.stderr);
    assert_eq!(blocked_json["code"], "conflict");

    let allowed_delete = cli_command(&temp)
        .args(["calendars", "delete", &calendar_id, "--cascade-events"])
        .output()
        .unwrap();
    assert!(allowed_delete.status.success());

    let listed = cli_command(&temp)
        .args(["events", "list"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed_json = read_json(&listed.stdout);
    assert!(listed_json["data"].as_array().unwrap().is_empty());
}
