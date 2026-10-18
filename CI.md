# Continuous integration

`.github/workflows/ci.yml` is the source of truth; this file only explains what it
does and why, so the reasoning survives a workflow edit.

Two jobs.

**`rust`** runs against a PostgreSQL service container, in this order:

1. `cargo fmt --all --check` — formatting is not a matter of taste here, it is a
   merge-conflict prevention measure.
2. `cargo clippy --workspace --all-targets -- -D warnings` — the workspace keeps
   zero warnings, so this is a hard gate rather than advice.
3. `cargo test --workspace` — the unit tests, which need no database. A test that
   needs one is marked `#[ignore]` on purpose, so this step passes on a laptop.
4. `cargo run -p migration -- up`, then `bootstrap` **twice**. Seeding is idempotent
   by design; a second run that changes something is the check.
5. `cargo test --workspace -- --include-ignored` — the integration tests, which are
   the ones a fake repository cannot make: real SQL, real constraints, and the two
   writes the application treats as atomic.
6. Roll every migration back, then re-apply. A migration that cannot be reversed is
   a migration that cannot be rolled forward either, and the deploy story depends on
   both directions working.

**`web`** type-checks, lints with Biome, and builds. The build also generates
`routeTree.gen.ts`, so a route file the plugin cannot parse fails here rather than
at deploy time.

## Running it locally

The same commands are in `README.md`'s Testing section. The integration tests need:

```bash
docker compose -f docker-compose.dev.yml up -d
export DATABASE_URL='postgres://postgres:postgres@127.0.0.1:5433/sudobot'
export JWT_SECRET='0123456789012345678901234567890123456789'
export DISCORD_CLIENT_ID=test DISCORD_CLIENT_SECRET=test
cargo run -p migration -- up
cargo test --workspace -- --include-ignored
```
