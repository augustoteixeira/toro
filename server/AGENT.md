# Server Agent Instructions

## Environment

- `cargo` is not on the default `PATH`; prefix commands with `export PATH="$HOME/.cargo/bin:$PATH"` or rely on the shell having it set.
- Do **not** background `cargo run` with `&` to test the server — it creates zombie processes. Use `timeout 3s cargo run 2>&1` instead to run briefly and capture output cleanly.

## Architecture

- `server/src/lib.rs` — public library crate. Contains:
  - `Db` struct (rocket_db_pools SQLite pool, pool name `"db"`)
  - `migrate()` — bootstraps `meta` table, reads `schema_version`, runs `migrations/*.sql`
  - `ensure_token()` — on first run, generates a random API token, prints it once, stores bcrypt hash in `meta`
  - `TokenAuthenticated` — Rocket request guard; validates `Authorization: Bearer <token>` against bcrypt hash in `meta`
  - `RateLimiter` — sliding window rate limiter (per IP); 10/min on POST, 20/min on GETs
  - `Reading` — hourly sensor row; derives `Serialize`, `Deserialize`, `FromRow`
  - `AggregatedBucket` — time-aggregated stats row used by all multi-day charts; fields: `label`, `{metric}_mean`, `{metric}_std` for temp/humidity/wind_speed/luminosity; `wind_direction_mean` (vector-averaged); `rainfall_sum`, `rainfall_max`
  - `aggregate_week/month/semester/triennium(key, readings)` — bucket readings into `AggregatedBucket` vecs
  - `generate_{day,week,month,semester,triennium}_json(pool, key)` — writes pre-computed JSON to `data/static/{span}/{key}.json`
  - `get_all_{dates,weeks,months,semesters,triennia}(pool)` — enumerate all distinct time keys in the DB
  - `semesters_containing(hour)` — returns all 6 semester keys whose window contains the given hour
  - `triennia_containing(hour)` — returns up to 3 triennium year keys containing the given hour
- `server/src/main.rs` — thin binary entrypoint. Uses `#[rocket::main]` (not `#[launch]`) so we can run logic between `ignite()` and `launch()`. Passes `--regenerate` flag to regenerate all static JSON without launching.
- `server/Rocket.toml` — database config: `data/db.sqlite`, port 8008
- `server/migrations/001-init.sql` — creates `hourly_readings` table, sets `schema_version = '1'`
- `server/data/` — runtime data dir. `db.sqlite` gitignored. `data/static/{day,week,month,semester,triennium}/` gitignored. `justfile` has `reset`, `generate`, `regenerate` recipes.
- `server/data/generate_fake.py` — Python script generating ~3 years of realistic Goiás weather data with gaps and sensor errors. Run via `just generate`.
- `server/tests/` — integration tests using in-memory SQLite pools. Test files: `test_migrate.rs`, `test_token.rs`, `test_auth.rs`, `test_readings.rs`, `test_day.rs`, `test_week.rs`, `test_rate_limit.rs`. Run with `cargo test`.
- `deployment/script/` — `update_default` (SSH deploy script), `release` (creates GitHub release with binary), `README.md` (supervisor + nginx setup).

## Time span keys

| Span | Key format | Granularity | Buckets |
|------|-----------|-------------|---------|
| Day | `YYYY-MM-DD` | hourly | 24 |
| Week | `YYYY-MM-DD` (Monday) | quarter-day | 28 |
| Month | `YYYY-MM` | daily | 28–31 |
| Semester | `YYYY-MM` | weekly | 26 |
| Triennium | `YYYY` | monthly | 36 |

Semesters overlap by 5 months (one per month start). Triennia overlap by 2 years (one per year start, covering Jan Y through Dec Y+2).

## Routes

| Method | Path | Description |
|--------|------|-------------|
| `GET`  | `/` | Landing page: one button per year |
| `POST` | `/readings` | Insert hourly reading (auth required); triggers JSON regeneration |
| `GET`  | `/day/<date>` | HTML page with UIkit tabs + Vega-Lite charts (24 hourly points) |
| `GET`  | `/week/<monday>` | HTML page with error bar charts (28 quarter-day buckets) |
| `GET`  | `/month/<month>` | HTML page with error bar charts (one point per day) |
| `GET`  | `/semester/<month>` | HTML page with error bar charts (26 weekly buckets) |
| `GET`  | `/triennium/<year>` | HTML page with error bar charts (36 monthly buckets) |
| `GET`  | `/api/day/<date>` | Raw hourly JSON |
| `GET`  | `/api/week/<monday>` | Aggregated week JSON |
| `GET`  | `/api/month/<month>` | Aggregated month JSON |
| `GET`  | `/api/semester/<month>` | Aggregated semester JSON |
| `GET`  | `/api/triennium/<year>` | Aggregated triennium JSON |

## Key Design Decisions

- **ISO-8601 text keys** for time: `hourly_readings.hour` is TEXT (e.g. `"2026-03-15T14"`), primary key. No relational date tables.
- **Write-through pre-computation**: each `POST /readings` triggers regeneration of all time-span JSON files for that reading. GET endpoints serve static files only — no DB queries on read.
- **`--regenerate` flag**: `cargo run -- --regenerate` regenerates all static JSON from DB and exits without launching the server.
- **Wind direction**: averaged via vector decomposition (speed as magnitude) rather than naive angle averaging.
- **Rainfall**: stored as `rainfall_sum` (total per period) and `rainfall_max` (peak hourly intensity) in aggregated buckets.
- **API token** (not password): auto-generated on first server start, stored as bcrypt hash in `meta` table under key `token_hash`. The plaintext is shown once and never stored.
- **`meta` table** is a general-purpose key-value store for app config (`schema_version`, `token_hash`).
- **`hourly_readings` columns**: `hour` (TEXT PK), `temperature`, `humidity`, `wind_speed`, `wind_direction`, `luminosity`, `rainfall` (all REAL).
- **Frontend stack**: UIkit (CDN) for tabs, Vega-Lite (CDN) for charts. Tab state is preserved in the URL hash (`#wind-direction` etc.) and propagated to all navigation links.
- **Deployment**: VPS behind nginx (HTTPS), process managed by supervisord. `ROCKET_SECRET_KEY` env var required in release mode (set in supervisor config).
