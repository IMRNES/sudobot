# Continuous integration

`.github/workflows/ci.yml` is the source of truth; this file only explains what it
does and why, so the reasoning survives a workflow edit.

`deploy.yml` **calls** this file rather than repeating it, so a pull request and a
push to main are gated by the same steps. `ci.yml` therefore has no `push` trigger
of its own: main is covered by the deploy run that calls it, and triggering on both
would run the whole suite twice for every main commit and leave two definitions of
"the checks" to drift apart.

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

## The deploy pipeline

`.github/workflows/deploy.yml` adds three jobs after the gate, all of which skip on a
pull request:

- **`web-artifact`** builds the dashboard. `VITE_API_BASE_URL` is empty on purpose:
  the SPA and the API are served from one origin, so every request is relative and
  CORS does not exist in production.
- **`build`** compiles the four release binaries and assembles the payload, asserting
  each executable and `web/index.html` exist *before* uploading. A build that produced
  three binaries then fails on the runner instead of after the old release has been
  stopped.
- **`deploy`** ships the payload and runs `scripts/deploy-direct.sh` on the host,
  which installs the release, applies migrations through the same secret path the
  service uses, flips the `current` symlink, and health-checks over HTTP. A failed
  health check restores the previous release.

`workflow_dispatch` runs the whole file from the dispatched ref, so the deploy job is
gated on `github.ref == 'refs/heads/main'` rather than on the event name — an
event-based gate would let a dispatch aimed at a branch deploy that branch over
production.

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
