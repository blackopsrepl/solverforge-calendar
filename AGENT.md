# AGENT.md

## Purpose

This repository contains a Linux-first Rust desktop calendar with two supported entrypoints:

- `solverforge-calendar`: ratatui TUI application
- `solverforge-calendar-cli`: non-interactive JSON CLI for agents and automation

## Commands

- `make build`: build both binaries
- `make run`: launch the TUI
- `make run-cli ARGS="calendars list"`: run the automation CLI
- `make test`: run all tests
- `make lint`: run formatting and clippy checks
- `make ci-local`: match the GitHub Actions CI workflow locally
- `make pre-release`: run release-oriented validation before cutting or tagging a version

Direct cargo commands used in CI:

- `cargo fmt --all -- --check`
- `cargo clippy --bins --tests -- -D warnings`
- `cargo build --bins`
- `cargo test`

## Repo map

- `src/main.rs`: TUI entrypoint
- `src/bin/solverforge-calendar-cli.rs`: CLI entrypoint
- `src/cli.rs`: typed CLI parsing, JSON responses, command dispatch, CLI tests
- `src/db.rs`: SQLite schema, migrations, CRUD helpers, default-calendar recovery
- `src/google/`: OAuth, sync fetch/apply logic, Google event translation
- `tests/cli.rs`: binary-level CLI integration tests
- `docs/wireframes/`: ASCII wireframes for the TUI and CLI surfaces

## Constraints

- Keep the CLI fully non-interactive. No prompts, no confirmation flows, no “choices”.
- Preserve `cargo run` as the TUI default path.
- Keep agent automation explicit through `solverforge-calendar-cli` and `scripts/solverforge-calendar-cli`.
- Prefer shared DB/business-rule fixes over CLI-only patches when behavior affects both the TUI and CLI.
- Tests must stay deterministic. Do not add live Google API or real keyring dependencies to automated tests.

## Change checklist

Before pushing changes:

1. Run `make lint`
2. Run `make test`
3. If binaries changed materially, run `make build`
4. Before tagging or pushing a release version, run `make pre-release`

If you touch the CLI contract, update:

- `README.md`
- `tests/cli.rs`
- `docs/wireframes/cli.md` when the command surface or response model changes
