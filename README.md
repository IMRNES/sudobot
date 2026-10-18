# sudobot

A Discord bot and its web dashboard, in one repository.

| Piece | Stack | What it is |
|---|---|---|
| `apps/iam` | Rust, Axum, SeaORM | The core: domain, use cases, repositories, HTTP handlers |
| `apps/gateway` | Rust, Axum | The dashboard API binary |
| `apps/bot` | Rust, serenity + poise | The Discord gateway binary |
| `apps/bootstrap` | Rust | One-shot seeding of the permission catalog and system roles |
| `web` | React, TanStack Router, Tailwind | The dashboard |
| `.migrations` | sea-orm-migration | The schema |
| `.config` | Rust | Environment configuration, read once per process |

The bot and the API are separate binaries that share one PostgreSQL database. That
split is deliberate: the bot holds a long-lived gateway connection, and redeploying
the dashboard must not drop it.

## Quick start

```bash
# 1. a database
docker compose -f docker-compose.dev.yml up -d

# 2. configuration
cp .env.example .env    # then fill in the Discord credentials

# 3. schema and seed data
cargo run -p migration -- up
cargo run -p bootstrap

# 4. run
cargo run -p gateway    # the dashboard API on :8080
cargo run -p bot        # the Discord bot
pnpm install && pnpm --filter web dev   # the dashboard on :5173
```

## Deploying

Production is **imrnes** (`sudobot.imrnes.team`), deployed by GitHub Actions on
every push to `main`. Nothing is built on the host: CI compiles four release
binaries plus the dashboard, and only those artifacts travel.

```
push to main
  -> ci.yml        fmt, clippy -D warnings, unit tests, migrations up/down vs Postgres
  -> build         cargo build --release, assemble + audit the payload
  -> deploy        scp the payload, run scripts/deploy-direct.sh on the host
```

The script installs `/opt/sudobot/releases/<sha>`, flips the `current` symlink,
applies migrations through the same secret path the service uses, restarts
`sudobot-gateway` and `sudobot-bot`, and health-checks the API over HTTP. A failed
check restores the previous release automatically.

Configuration comes from Bitwarden Secrets Manager via `bws-exec`, as
`sudobot_<KEY>` keys, so adding a variable is `bws secret create` and nothing else
— no unit edit and no file on the host.

The level ladder is provisioned at startup from `DISCORD_PROVISION_GUILD_ID`, so a
deploy reproduces the guild's roles rather than depending on a manual slash
command. See `ARCHITECTURE.md` for why each rung is permissionless and why level 0
needs a join-time grant.

`docker-compose.yml` remains a self-contained alternative for running the whole
stack somewhere without CI. It applies migrations as a one-shot service and seeds
with `bootstrap`, then starts the API, the bot and nginx — all three waiting on the
migration step, so a failed schema change stops the deploy rather than starting an
application against a half-built database.

## Configuration

Every setting is read once, at startup, by `sudobot-config`. A missing or malformed
value stops the process with a readable message rather than failing later on the
first request that needs it.

| Variable | Required | Notes |
|---|---|---|
| `DATABASE_URL` | yes | `postgres://user:pass@host:port/db` |
| `JWT_SECRET` | yes | At least 32 bytes. `openssl rand -hex 32` |
| `DISCORD_BOT_TOKEN` | for the bot | Developer Portal → Bot |
| `DISCORD_CLIENT_ID` / `DISCORD_CLIENT_SECRET` | for the dashboard | Developer Portal → OAuth2 |
| `DISCORD_DEV_GUILD_ID` | no | Registers slash commands to one guild, instantly |
| `BOOTSTRAP_SUPER_ADMIN_DISCORD_ID` | no | Promoted by `bootstrap` |
| `CORS_ORIGINS` | production | Comma-separated. Empty means "any localhost" in development |
| `WEB_ORIGIN` | no | Where the OAuth callback returns to |

See `.env.example` for the full list with defaults.

### Dashboard sign-in needs two things only the Developer Portal can set

Both are deliberately optional at startup, so a bot-only deployment runs without
them — but until they exist, `POST /auth/discord/authorize` answers `503` with
`code: "not_configured"` and the dashboard shows the message instead of a login
button that goes nowhere.

1. **The client secret.** Discord shows `DISCORD_CLIENT_SECRET` exactly once, when
   it is generated. It is not readable back over the API — `GET /applications/@me`
   has no such field — so a lost secret must be reset in the portal.
2. **The redirect URI.** Add
   `https://<host>/api/v1/iam/auth/discord/callback` to
   Developer Portal → OAuth2 → Redirects. This is also not settable over the API:
   `PATCH /applications/@me` accepts `redirect_uris`, returns `200`, and leaves the
   list empty. `GET` will keep reporting `[]` until a human adds it.

## Schema changes

Migrations are a separate binary, not something the server does on boot. A schema
change is a deliberate deploy step with its own exit code.

```bash
cargo run -p migration -- up        # apply everything pending
cargo run -p migration -- down 1    # roll one back
cargo run -p migration -- status    # what has run
cargo run -p migration -- fresh     # drop and rebuild (development only)
```

Migrations live in `.migrations/src/`, one file per change, named
`m{YYYYMMDD}_{seq}_{description}.rs` and registered in `Migrator::migrations()` in
the order they must run.

## How the code is organised

`apps/iam` is a library crate with four layers, and the arrows only point inward:

```
presentation ──▶ application ──▶ domain
infrastructure ─▶ domain          (implements the traits domain declares)
presentation ──▶ infrastructure   (only to assemble AppState)
```

- **`domain/`** — entities, repository traits, errors, and the rules that decide
  what is allowed. No HTTP, no SeaORM, no Discord. If a rule lives anywhere else it
  is a bug, because both the bot and the API must apply it identically.
- **`application/`** — use cases. One verb each, generic over the traits they need,
  constructed per request. They orchestrate; they do not decide.
- **`infrastructure/`** — SeaORM repositories and the Argon2/JWT/Discord adapters.
  The only layer that knows a database exists.
- **`presentation/`** — Axum handlers, DTOs, middleware. Three lines of work each:
  authorize, convert, delegate.

Adding a feature means walking that list in order. `API.md` is the HTTP contract.

## Conventions worth knowing before you read the code

- **Snowflakes are strings in JSON.** They exceed 2^53, and a JavaScript client
  would silently round them otherwise.
- **Timestamps are UTC everywhere**; SeaORM models carry `DateTimeWithTimeZone` and
  convert at the adapter boundary.
- **Every mutation accepts an idempotency key** where Discord might retry, because a
  retried interaction must not pay a member twice.
- **Errors have one mapping.** Every domain error converts into `AppError`, which
  owns the status code decision. Handlers write `?` and get the right status.
- **`impl Future` in traits, not `#[async_trait]`.** Edition 2024.

## Testing

```bash
cargo test --workspace                     # unit tests, no database needed
cargo clippy --workspace --all-targets     # kept at zero warnings
cargo fmt --all --check
pnpm --filter web exec tsc --noEmit
pnpm --filter web exec biome check .
```

Integration tests live in `apps/iam/tests/` and need a migrated database. They are
marked `#[ignore]`, so `cargo test` passes on a machine without one:

```bash
docker compose -f docker-compose.dev.yml up -d
export DATABASE_URL='postgres://postgres:postgres@127.0.0.1:5433/sudobot'
export JWT_SECRET='0123456789012345678901234567890123456789'
export DISCORD_CLIENT_ID=test DISCORD_CLIENT_SECRET=test
cargo run -p migration -- up
cargo test --workspace -- --include-ignored
```

`repositories.rs` covers the SQL a fake cannot reach — an atomic balance update, the
guarded stock decrement, idempotency. `http_api.rs` drives the assembled router
through its extractor stack: the public/private tier split, the tenant boundary, the
validation messages.

## Documentation

- `API.md` — every route, its shape, and the permission it needs.
- `ARCHITECTURE.md` — the decisions behind the layering, and the trade-offs taken.
