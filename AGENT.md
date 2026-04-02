# Agent Instructions

For the server side, we take inspiration at orbitask at https://github.com/augustoteixeira/orbitask

## Core Principles

- Make **atomic changes only**. One small, focused change at a time.
- Do **not** go beyond what was explicitly requested. If a task says "add X", add X and nothing else.
- Do **not** refactor, rename, reorganize, or "improve" anything that was not part of the request.
- Do **not** add dependencies, files, or boilerplate unless explicitly asked.
- Ask for clarification before making any assumption that would affect the implementation.

## Project Overview

This is a DIY weather station project with two components:

1. **ESP32 firmware** — written in Rust, using `esp-idf` with FreeRTOS for async support and verified TLS.
2. **Web server** — written in Rust using the Rocket framework, serving the web frontend.

## Development Approach

- Progress is **intentionally slow and deliberate**.
- Each change must be justified by an explicit instruction.
- Prefer correctness and clarity over cleverness or completeness.
- Do not anticipate future steps or scaffold ahead.

## Server Development Workflow

The server is built incrementally following the plan in `server/TODO.md`. For each item:

1. **Pick** — the user selects the next TODO item to tackle.
2. **Discuss** — the agent proposes an approach; the user reviews and adjusts.
3. **Agree** — both sides confirm the strategy before any code is written.
4. **Implement** — the agent writes the code.
5. **Review** — verify the change builds, passes tests, or runs correctly.
6. **Commit** — create a focused commit for the completed item.

## Working Dynamics

- The user drives all decisions; agents propose and wait for confirmation.
- When a task requires interactive commands (e.g. `cargo generate`), instruct the user to run them and wait for the result before proceeding.
- Diagnose problems by actually running commands and reading output — do not guess.
- To flash the device, use `esp/run_until.sh <sentinel>` instead of running `cargo espflash flash --monitor` directly. The script runs the flash command as a background process and exits cleanly once the sentinel string appears in the serial output. Example: `./run_until.sh "BOOT_OK"`. See `esp/run_until.sh` for full usage.
- Known environment issues to be aware of:
  - `cargo` is not on the default `PATH`; prefix commands with `export PATH="$HOME/.cargo/bin:$PATH"` or rely on the shell having it set.
  - `cargo-espflash` 4.x fails to compile; use version 3.3.0.
  - `libclang-dev` and `libudev-dev` must be installed via `apt` before building the esp crate.
  - `ldproxy` must be installed via `cargo install ldproxy`.
  - The user must be in the `dialout` group to flash over USB.
  - Do **not** background `cargo run` with `&` to test the server — it creates zombie processes. Use `timeout 3s cargo run 2>&1` instead to run briefly and capture output cleanly.

## Server Architecture (current state)

- `server/src/lib.rs` — public library crate. Contains:
  - `Db` struct (rocket_db_pools SQLite pool, pool name `"db"`)
  - `migrate()` — bootstraps `meta` table, reads `schema_version`, runs `migrations/*.sql`
  - `ensure_token()` — on first run, generates a random API token, prints it once, stores bcrypt hash in `meta`
  - `TokenAuthenticated` — Rocket request guard; validates `Authorization: Bearer <token>` against bcrypt hash in `meta`
- `server/src/main.rs` — thin binary entrypoint. Uses `#[rocket::main]` (not `#[launch]`) so we can run logic between `ignite()` and `launch()`.
- `server/Rocket.toml` — database config: `data/db.sqlite`
- `server/migrations/001-init.sql` — creates `hourly_readings` table, sets `schema_version = '1'`
- `server/data/` — runtime data dir. `db.sqlite` is gitignored. Contains `fake_dump.sql` and a `justfile` with `reset` / `fill` recipes.
- `server/tests/` — integration tests using in-memory SQLite pools (`tests/common.rs` helper). Test files: `test_migrate.rs`, `test_token.rs`, `test_auth.rs`. Run with `cargo test`.

## Key Design Decisions

- **ISO-8601 text keys** for time: `hourly_readings.hour` is TEXT (e.g. `"2026-03-15T14"`), primary key. No separate year/month/day entity tables.
- **Summary tables** (daily/monthly) will be added later as separate flat tables keyed by ISO-8601 strings — not as foreign-key relationships.
- **API token** (not password): auto-generated on first server start, stored as bcrypt hash in `meta` table under key `token_hash`. The plaintext is shown once and never stored.
- **`meta` table** is a general-purpose key-value store for app config (`schema_version`, `token_hash`, future settings).
- **`hourly_readings` columns**: `hour` (TEXT PK), `temperature`, `humidity`, `wind_speed`, `wind_direction`, `luminosity`, `rainfall` (all REAL). No `id` or `token_id` column — `token_id` lives in `meta`.
