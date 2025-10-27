# Country Currency & Exchange API (Rust + Axum + MySQL)

Fetches countries and currency exchange rates, computes an estimated GDP, caches to MySQL, serves CRUD + a summary PNG.

## Stack
- Rust (Axum 0.7, SQLx, Reqwest, Tokio)
- MySQL 8
- Docker Compose (optional)
- Image generation: image/imageproc + ab_glyph (TTF font)

---

## Endpoints

- `POST /countries/refresh` — fetch countries + rates, compute `estimated_gdp`, upsert, build summary image
- `GET /countries` — list (filters: `?region=`, `?currency=`; sort: `?sort=gdp_desc|gdp_asc|name_asc|population_desc`; paging: `?page=&limit=`)
- `GET /countries/:name` — fetch one by case-insensitive name
- `DELETE /countries/:name` — delete by name
- `GET /status` — total countries + last refresh timestamp
- `GET /countries/image` — serve the generated PNG summary
- `GET /healthz` — DB health check (`SELECT 1`)

### Error shape
- `400` → `{"error":"Validation failed","details":{...}}`
- `404` → `{"error":"Country not found"}`
- `503` → `{"error":"External data source unavailable","details":"..."}`
- `500` → `{"error":"Internal server error","details":"..."}`

---

## Prerequisites

- Rust ≥ **1.81** (`rustup update stable`)
- MySQL 8 (local **or** via Docker)
- (For tests) Docker Desktop/Engine running

---

## Project Layout

src/
main.rs
config/ # AppConfig, AppState
routes/ # router()
handlers/ # countries handlers
services/ # refresh_service (fetch, compute, upsert, image)
models/ # Country struct
types/ # external API types
utils/ # error, image generation
migrations/
0001_init.sql # schema
assets/
DejaVuSans.ttf # font used by image generator (replace with a real TTF)


> The font is embedded at compile time. Replace `assets/DejaVuSans.ttf` with a real TTF (e.g., DejaVu Sans/Noto/Roboto) so `/countries/image` renders text.

---

## Running Locally (Cargo)

1) **Create `.env`** (dev defaults):
```env
RUST_LOG=info
PORT=8080
DATABASE_URL=mysql://appuser:apppass@127.0.0.1:3306/countrydb
EXTERNAL_TIMEOUT_MS=12000
BASE_CURRENCY=USD
SUMMARY_IMAGE_PATH=cache/summary.png

Start/prepare MySQL (ensure DB & user exist):
mysql -h 127.0.0.1 -u root -p -e "
  CREATE DATABASE IF NOT EXISTS countrydb
    CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;
  CREATE USER IF NOT EXISTS 'appuser'@'127.0.0.1' IDENTIFIED BY 'apppass';
  GRANT ALL PRIVILEGES ON countrydb.* TO 'appuser'@'127.0.0.1';
  FLUSH PRIVILEGES;
"

Run migration (script provided below):
chmod +x scripts/migrate_local.sh
./scripts/migrate_local.sh

Run the API:
cargo run
# Logs should include: "✅ Database connected" and "🚀 Listening on http://0.0.0.0:8080"

Smoke test:
curl -s -X POST http://localhost:8080/countries/refresh | jq .
curl -s "http://localhost:8080/countries?region=Africa&sort=gdp_desc" | jq .
curl -s http://localhost:8080/status | jq .
curl -s -o summary.png http://localhost:8080/countries/image

Running with Docker Compose
Create .env.docker:
RUST_LOG=info
API_PORT=8080
EXTERNAL_TIMEOUT_MS=12000
BASE_CURRENCY=USD
SUMMARY_IMAGE_PATH=cache/summary.png

MYSQL_HOST=db
MYSQL_PORT=3306
MYSQL_DATABASE=countrydb
MYSQL_USER=appuser
MYSQL_PASSWORD=apppass
MYSQL_ROOT_PASSWORD=rootpass

Start:
docker compose --env-file .env.docker up -d --build
# API on http://localhost:8080

Migration (first run auto-applies if volume empty). To force/manual apply:
chmod +x scripts/migrate_docker.sh
./scripts/migrate_docker.sh

Smoke test (same as local):
curl -s -X POST http://localhost:8080/countries/refresh | jq .
curl -s http://localhost:8080/status | jq .

Integration Tests
Tests spin up MySQL (testcontainers) + mock external APIs (wiremock), run the real router in-process.
rustup update stable
docker info   # ensure Docker is running

cargo test -- --nocapture




