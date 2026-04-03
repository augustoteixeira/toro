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
  - `RateLimiter` — sliding window rate limiter (per IP); 10/min on POST, 20/min on GETs
  - `Reading` — hourly sensor row; derives `Serialize`, `Deserialize`, `FromRow`
  - `AggregatedBucket` — time-aggregated stats row used by week and month charts; fields: `label`, `{metric}_mean`, `{metric}_std` for temp/humidity/wind_speed/luminosity; `wind_direction_mean` (vector-averaged); `rainfall_sum`, `rainfall_max`
  - `insert_reading`, `get_readings_for_day/week/month` — DB queries
  - `aggregate_week(monday, readings)` — 28 quarter-day buckets (Mon 0-6 … Sun 18-24)
  - `aggregate_month(month, readings)` — one bucket per day
  - `generate_day/week/month_json(pool, key)` — writes pre-computed JSON to `data/static/{day|week|month}/{key}.json`
  - `get_all_dates/weeks/months(pool)` — enumerate all distinct time keys in the DB
  - `monday_of(hour)`, `month_of(hour)` — derive week/month key from an hour string
- `server/src/main.rs` — thin binary entrypoint. Uses `#[rocket::main]` (not `#[launch]`) so we can run logic between `ignite()` and `launch()`. Passes `--regenerate` flag to regenerate all static JSON without launching.
- `server/Rocket.toml` — database config: `data/db.sqlite`, port 8008
- `server/migrations/001-init.sql` — creates `hourly_readings` table, sets `schema_version = '1'`
- `server/data/` — runtime data dir. `db.sqlite` gitignored. `data/static/{day,week,month}/` gitignored. `justfile` has `reset`, `generate`, `regenerate` recipes.
- `server/data/generate_fake.py` — Python script generating ~3 years of realistic Goiás weather data with gaps and sensor errors. Run via `just generate`.
- `server/tests/` — integration tests using in-memory SQLite pools. Test files: `test_migrate.rs`, `test_token.rs`, `test_auth.rs`, `test_readings.rs`, `test_day.rs`, `test_week.rs`, `test_rate_limit.rs`. Run with `cargo test`.
- `deployment/script/` — `update_default` (SSH deploy script), `release` (creates GitHub release with binary), `README.md` (supervisor + nginx setup).

## Routes

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/readings` | Insert hourly reading (auth required); triggers JSON regeneration |
| `GET`  | `/day/<date>` | HTML page with UIkit tabs + Vega-Lite charts (one per metric, 24 hourly points) |
| `GET`  | `/week/<monday>` | HTML page with error bar charts (28 quarter-day buckets) |
| `GET`  | `/month/<month>` | HTML page with error bar charts (one point per day) |
| `GET`  | `/api/day/<date>` | Raw hourly JSON from `data/static/day/<date>.json` |
| `GET`  | `/api/week/<monday>` | Aggregated week JSON from `data/static/week/<monday>.json` |
| `GET`  | `/api/month/<month>` | Aggregated month JSON from `data/static/month/<month>.json` |

## Key Design Decisions

- **ISO-8601 text keys** for time: `hourly_readings.hour` is TEXT (e.g. `"2026-03-15T14"`), primary key. No relational date tables.
- **Write-through pre-computation**: each `POST /readings` triggers regeneration of the day, week, and month JSON files. GET endpoints serve static files only — no DB queries on read.
- **`--regenerate` flag**: `cargo run -- --regenerate` regenerates all static JSON from DB and exits without launching the server.
- **Wind direction**: averaged via vector decomposition (speed as magnitude) rather than naive angle averaging.
- **Rainfall**: stored as `rainfall_sum` (total per period) and `rainfall_max` (peak hourly intensity) in aggregated buckets.
- **API token** (not password): auto-generated on first server start, stored as bcrypt hash in `meta` table under key `token_hash`. The plaintext is shown once and never stored.
- **`meta` table** is a general-purpose key-value store for app config (`schema_version`, `token_hash`).
- **`hourly_readings` columns**: `hour` (TEXT PK), `temperature`, `humidity`, `wind_speed`, `wind_direction`, `luminosity`, `rainfall` (all REAL).
- **Frontend stack**: UIkit (CDN) for tabs, Vega-Lite (CDN) for charts. Day page renders all charts on load. Week/month pages use error bars for most metrics.
- **Deployment**: VPS behind nginx (HTTPS), process managed by supervisord. `ROCKET_SECRET_KEY` env var required in release mode (set in supervisor config).
