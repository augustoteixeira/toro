# Server TODO

## 1. Project scaffolding
- [ ] Create `server/` Rust crate with Rocket, `rocket_db_pools`/sqlx (SQLite), and maud

## 2. Database setup
- [ ] Add `Rocket.toml` with database path
- [ ] Add migration scaffolding: `meta` table, `schema_version` check at startup
- [ ] Define and run initial migration: `readings` table (`id`, `timestamp`, `token_id`, payload columns)

## 3. Authentication
- [ ] Store hashed API tokens in the database
- [ ] Implement `TokenAuthenticated` request guard (validates `Authorization: Bearer <token>` header)
- [ ] First-run token generation (prompt or CLI command)

## 4. POST endpoint
- [ ] `POST /readings` — ESP submits sensor data; guarded by `TokenAuthenticated`; inserts a row

## 5. GET endpoints (time-scale based)
- [ ] `GET /year/<year>` — render yearly summary (e.g. `/year/2026`)
- [ ] `GET /month/<year-month>` — render monthly readings (e.g. `/month/2026-03`)
- [ ] `GET /day/<year-month-day>` — render daily readings (e.g. `/day/2026-06-28`)
- [ ] `GET /` — landing page (redirect to current day, or render latest readings)

## 6. Error handling
- [ ] Flash messages for user-facing errors
- [ ] 401 catcher for unauthenticated requests

## 7. Hardening
- [ ] Rate limiting on `POST /readings` (per-token or per-IP)

## 8. Deployment
- [ ] Deployment config (supervisord or Docker)
