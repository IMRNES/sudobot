# The API, the bot and the migration runner, built from one workspace.
#
# Build context is the repository root, because every binary needs every crate in
# the workspace. One image carries all three — they share the whole dependency tree,
# so building them separately would compile tokio, sea-orm and serenity three times
# — and each compose service picks one with `command`.
#
#   docker build -t sudobot .

# ---------------------------------------------------------------------------
# Build stage
# ---------------------------------------------------------------------------
FROM rust:1.99-slim-bookworm AS builder

# rustls everywhere means no OpenSSL to install; the rest is what sqlx's
# postgres driver and reqwest need to link.
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy the manifests first so the dependency build is cached independently of the
# source. Without this layer split, every source edit rebuilds all of tokio.
COPY Cargo.toml Cargo.lock ./
COPY .config/Cargo.toml .config/
COPY .migrations/Cargo.toml .migrations/
COPY apps/iam/Cargo.toml apps/iam/
COPY apps/gateway/Cargo.toml apps/gateway/
COPY apps/bot/Cargo.toml apps/bot/
COPY apps/bootstrap/Cargo.toml apps/bootstrap/

# Placeholder sources so `cargo fetch` can resolve the workspace, then the real
# ones overwrite them.
RUN mkdir -p .config/src .migrations/src/bin apps/iam/src apps/gateway/src \
        apps/bot/src apps/bootstrap/src \
    && echo '' > .config/src/lib.rs \
    && echo '' > .migrations/src/lib.rs \
    && echo 'fn main() {}' > .migrations/src/bin/migrate.rs \
    && echo '' > apps/iam/src/lib.rs \
    && echo 'fn main() {}' > apps/gateway/src/main.rs \
    && echo 'fn main() {}' > apps/bot/src/main.rs \
    && echo 'fn main() {}' > apps/bootstrap/src/main.rs \
    && cargo fetch --locked

COPY .config .config
COPY .migrations .migrations
COPY apps apps

# All three binaries in one pass: they share every dependency, so a per-binary
# build would compile the same crates three times over.
RUN cargo build --release --locked \
    --bin gateway --bin bot --bin bootstrap --bin migrate

# ---------------------------------------------------------------------------
# Runtime stage
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# ca-certificates is not optional: both binaries talk to Discord over TLS, and
# without the trust store every request fails with an unhelpful handshake error.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Unprivileged by default. Nothing here needs to write to the filesystem.
RUN useradd --system --create-home --uid 10001 sudobot
USER sudobot

WORKDIR /app

COPY --from=builder /build/target/release/gateway /usr/local/bin/gateway
COPY --from=builder /build/target/release/bot /usr/local/bin/bot
COPY --from=builder /build/target/release/bootstrap /usr/local/bin/bootstrap
COPY --from=builder /build/target/release/migrate /usr/local/bin/migrate

ENV APP_ENV=production \
    APP_HOST=0.0.0.0 \
    APP_PORT=8080

EXPOSE 8080

# Overridden per service in compose; the API is the useful default.
CMD ["gateway"]
