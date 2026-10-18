# sudobot

A Discord bot and its web dashboard: Rust backend (Axum + SeaORM, clean
architecture), React dashboard, one PostgreSQL database between them.

## Commands

```bash
# Rust
cargo check --workspace                    # fast; use this while working
cargo test --workspace                     # unit tests, no database needed
cargo clippy --workspace --all-targets     # must stay at zero warnings
cargo fmt --all

# with a database (docker compose -f docker-compose.dev.yml up -d first)
cargo run -p migration -- up
cargo run -p bootstrap                     # idempotent; safe to re-run
DATABASE_URL=... cargo test --workspace -- --include-ignored

# Frontend
pnpm --filter web dev
pnpm --filter web exec tsc --noEmit
pnpm --filter web exec biome check .
```

Run one binary: `cargo run -p gateway` (API, :8080), `cargo run -p bot`,
`cargo run -p bootstrap`. Configuration comes from `.env` — see `.env.example`;
every value is read once at startup and a bad one stops the process with a
readable message.

## Layout

| Path | What it is |
|---|---|
| `apps/iam/` | The library: `domain/` → `application/` → `presentation/`, with `infrastructure/` implementing the domain's traits |
| `apps/gateway/` | The API binary — assembles `AppState`, runs the router |
| `apps/bot/` | The Discord binary — poise framework, gateway events, autorole sweeper |
| `apps/bootstrap/` | One-shot seeding of the permission catalog and system roles |
| `.migrations/` | Schema, one file per change, plus the `migrate` binary |
| `.config/` | Environment configuration, read once |
| `web/` | The dashboard |

## Rules this codebase holds to

These are not preferences; breaking them is a bug.

- **`domain/` depends on nothing.** No `axum`, no `sea_orm`, no `serenity`. It
  declares entities, repository traits and errors; the outer layers satisfy them.
  A rule in a handler instead of the domain is a bug, because the bot and the API
  would then apply it differently.
- **Use cases are generic over traits and built per request** from `AppState`.
  Never name a concrete `SeaOrm*` type in `application/`. Construct with
  `XxxUseCase::new(state.a.clone(), state.b.clone())` inside the handler.
- **`AppError` owns the status-code decision.** Add one `From` arm for a new domain
  error; never decide a status anywhere else, and never put an internal detail in
  an `AppError::Internal` message.
- **Snowflakes are strings in JSON.** They exceed 2^53; a browser rounds them
  otherwise. Use `#[serde(with = "snowflake")]` on every `u64` that is a Discord id.
- **Time comes from `Clock`**, never `Utc::now()` inside a use case — the cooldowns
  and schedules are only testable if a test can decide the time.
- **Timestamps:** `DateTimeWithTimeZone` in SeaORM models, `DateTime<Utc>` in the
  domain, converted in `infrastructure/repository/mapping.rs`.
- **`impl Future` in repository traits**, not `#[async_trait]` (edition 2024).
- **Money only moves through `apply_transaction`.** It re-reads the balance inside
  the transaction; a balance computed from an earlier read is how currency gets
  duplicated.
- **Retryable mutations take an idempotency key.** Discord retries interactions.
- **In tests, `unwrap`/`expect` are allowed** and each test target says so at file
  level. In production code they are warnings — do not silence them.
- `unsafe_code` is `forbid`, so no `set_var` in tests either.

## Where to look first

- `API.md` — every route, its shape, and the permission it needs.
- `ARCHITECTURE.md` — why the code is shaped this way, and a **Known gaps** section
  listing what is deliberately unfinished. Read that before assuming something is
  an oversight.
- `CI.md` and `.github/workflows/ci.yml` — what must pass.

## Gotchas that cost time already

- **`zod-rs` derive needs its attributes.** A struct deriving `ZodSchema` with no
  `#[zod(...)]` fields compiles fine and validates *nothing*. The `zod_rs` feature
  named `axum` does not provide an extractor; `presentation/extract.rs` has the
  local `ValidatedRequest` trait and the `validated_request!` macro that wire it up,
  and every request type needs one line of it.
- **The derive cannot express `Option<Option<T>>`** (absent vs. explicit null), so
  the three patch DTOs that need it (`UpdateWelcomeConfigRequest`,
  `UpdateLevelingConfigRequest`, `UpdateShopItemRequest`) implement `ValidatedRequest`
  by hand.
- **`jsonwebtoken` 11 panics at runtime without a crypto feature.** `rust_crypto` is
  set in the workspace manifest; removing it compiles and then fails on the first
  token.
- **poise 0.7 has no `poise::Event`, no `register_global_commands`, and no
  `builtins::help`.** It is `serenity::FullEvent`, `poise::builtins::register_globally`,
  and a hand-rendered help command.
- **`poise::FrameworkContext::user_data()` is `async`**, and a `#[poise::command]`
  function cannot be called as a plain async fn — poise's macro rewrites its
  signature.
- **`serenity::Http` is not `Clone`**; `Data` holds an `Arc<Http>` and `Data`
  implements `EventHandler` itself.
- **An axum layer applies to every route below it.** Mounting the auth layer on the
  merged router made `/health` and the login start return 401. `attach_auth` records
  the caller and never rejects; `AuthenticatedUser` is the extractor that refuses.
- **A handler calling a repository method needs the trait in scope** — the method is
  not inherent, and the error ("no method named `find_by_id`") points at the call
  site rather than the missing import.
