# Toro — Server

Rocket-based web server for the Toro weather station. Receives hourly sensor readings from an ESP32 and serves interactive charts at multiple time scales.

## Architecture

The server is split into a library crate (`src/lib.rs`) and a thin binary entrypoint (`src/main.rs`).

### Storage

- **Database**: SQLite at `data/db.sqlite` (path configured in `Rocket.toml`)
- **Static JSON**: pre-computed chart data written to `data/static/{day,week,month,semester,triennium}/`

On every authenticated `POST /readings`, the server inserts the row into the database and regenerates the static JSON files for all time spans that include that reading. GET endpoints serve those static files — no database queries on read.

### Time spans

| Span | Key format | Granularity | Example |
|------|-----------|-------------|---------|
| Day | `YYYY-MM-DD` | hourly (24 points) | `2025-01-15` |
| Week | `YYYY-MM-DD` (Monday) | quarter-day (28 points) | `2025-01-13` |
| Month | `YYYY-MM` | daily (28–31 points) | `2025-01` |
| Semester | `YYYY-MM` | weekly (26 points) | `2025-01` |
| Triennium | `YYYY` | monthly (36 points) | `2023` |

Semesters and triennia overlap: one semester per month start, one triennium per year start (each spanning 3 years from Jan 1).

### Routes

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/` | — | Landing page: one button per year |
| `POST` | `/readings` | Bearer token | Insert hourly reading, regenerate JSON |
| `GET` | `/day/<date>` | — | Day chart page |
| `GET` | `/week/<monday>` | — | Week chart page |
| `GET` | `/month/<month>` | — | Month chart page |
| `GET` | `/semester/<month>` | — | Semester chart page |
| `GET` | `/triennium/<year>` | — | Triennium chart page |
| `GET` | `/api/day/<date>` | — | Raw hourly JSON |
| `GET` | `/api/week/<monday>` | — | Aggregated week JSON |
| `GET` | `/api/month/<month>` | — | Aggregated month JSON |
| `GET` | `/api/semester/<month>` | — | Aggregated semester JSON |
| `GET` | `/api/triennium/<year>` | — | Aggregated triennium JSON |

### Sensors

Each hourly reading contains: `temperature` (°C), `humidity` (%), `wind_speed` (km/h), `wind_direction` (°), `luminosity` (lux), `rainfall` (mm).

Aggregated buckets use mean ± stddev for most metrics, vector-averaged direction for wind, and sum + max for rainfall.

## First run

```bash
cd server
cargo run
```

On first start the server:
1. Creates `data/db.sqlite` and runs migrations
2. Generates a random API token, prints it once, and stores the bcrypt hash in the `meta` table

**Save the token** — it is shown only once and is required by the ESP32.

Subsequent starts skip token generation silently.

## Development workflow

```bash
cd server/data

just reset       # delete db.sqlite and all generated JSON files
cd ..
cargo run        # migrate + generate token (Ctrl+C after token is shown)
cd data
just generate    # insert ~3 years of fake Goiás weather data
just regenerate  # = cargo run -- --regenerate (generates all static JSON)
cd ..
cargo run        # serve at http://localhost:8008
```

### Regenerating static files

After changes to chart templates or aggregation logic, regenerate all JSON without launching the server:

```bash
cargo run -- --regenerate
```

## Testing

```bash
cargo test
```

Integration tests use in-memory SQLite pools. Test files:

| File | Coverage |
|------|----------|
| `test_migrate.rs` | Migration, schema version |
| `test_token.rs` | First-run token generation |
| `test_auth.rs` | `TokenAuthenticated` request guard |
| `test_readings.rs` | `POST /readings`, JSON regeneration trigger |
| `test_day.rs` | Day queries, `generate_day_json` |
| `test_week.rs` | Week/month/semester/triennium aggregation and JSON generation |
| `test_rate_limit.rs` | Rate limiter logic |

## Deployment

See [`../deployment/script/README.md`](../deployment/script/README.md) for supervisor + nginx setup and the upgrade workflow.

Key points:
- Requires `ROCKET_SECRET_KEY` env var in release mode (set in supervisor config)
- Server listens on port **8008** by default
- Binary is compiled locally and uploaded via the `release` + `update` scripts
