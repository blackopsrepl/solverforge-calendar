# CLI Wireframe

## Command shape

```text
solverforge-calendar-cli <group> <action> [flags]
```

Supported groups:

- `calendars`
- `projects`
- `events`
- `dependencies`
- `google sync`

## Success response

```json
{
  "status": "ok",
  "data": { "...": "..." }
}
```

## Error response

```json
{
  "status": "error",
  "code": "validation_error",
  "message": "human-readable explanation"
}
```

## Behavioral notes

- Parsing is strict: unknown flags and malformed values fail fast.
- Destructive commands require flags instead of prompts.
- `calendars delete` cannot remove the last active calendar.
- `google sync` is non-interactive and must never depend on TUI state.
