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

    let calendars = cli_command(&temp)
        .args(["calendars", "list"])
        .output()
        .unwrap();
    assert!(calendars.status.success());
    let calendars_json = read_json(&calendars.stdout);
    let calendar_id = calendars_json["data"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

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
