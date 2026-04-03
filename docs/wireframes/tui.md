# TUI Wireframe

```text
┌──────────────── SolverForge Calendar ────────────────┬──────────── Sidebar ────────────┐
│ Month / Week / Day / Agenda                          │ Calendars                        │
│ Date range + sync status                             │  [x] Personal                    │
├──────────────────────────────────────────────────────┤  [x] Work                        │
│                                                      │  [ ] Travel                      │
│ Main calendar surface                                ├──────────────────────────────────┤
│                                                      │ Projects                         │
│ - Month: 5-week grid                                 │  Launch                          │
│ - Week: hourly time grid                             │  Planning                        │
│ - Day: single-day schedule                           ├──────────────────────────────────┤
│ - Agenda: sorted upcoming events                     │ Selected event summary           │
│                                                      │  Title                           │
│                                                      │  Time                            │
│                                                      │  Project / dependency hints      │
├──────────────────────────────────────────────────────┴──────────────────────────────────┤
│ Status bar: key hints, transient errors, Google auth/sync state                       │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

## Interaction notes

- Event creation/editing depends on at least one active calendar.
- Sidebar visibility controls filter the rendered event set.
- Google auth and sync status should remain visible without leaving the main workflow.
