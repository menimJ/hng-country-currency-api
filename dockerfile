# ---------- Build stage ----------
FROM rust:1.81-bookworm AS builder
WORKDIR /app

# Pre-copy manifests + migrations + assets so the sqlx macro and include_bytes! can see them
COPY Cargo.toml Cargo.lock* ./
COPY migrations ./migrations
COPY assets ./assets

# Prime the cargo cache
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs && cargo build --release || true

# Now copy real source and build
COPY src ./src
RUN cargo build --release

# ---------- Runtime stage ----------
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# Minimal runtime deps (TLS, timezone)
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata \
  && rm -rf /var/lib/apt/lists/*

# Copy the binary
COPY --from=builder /app/target/release/country-currency-api /usr/local/bin/country-currency-api

# Writable cache dir for /countries/image
RUN mkdir -p /app/cache && \
    useradd -u 10001 -r -s /usr/sbin/nologin appuser && \
    chown -R appuser:appuser /app

# Sensible defaults (override via Compose/ENV)
ENV RUST_LOG=info \
    PORT=8080 \
    SUMMARY_IMAGE_PATH=/app/cache/summary.png

EXPOSE 8080
USER appuser
ENTRYPOINT ["/usr/local/bin/country-currency-api"]
