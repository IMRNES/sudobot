# Architecture

Why the code is shaped the way it is, and which trade-offs were taken deliberately.
If you are looking for *what* a thing does, `README.md` and `API.md` answer that;
this file answers *why*.

## Two binaries, one database

The bot and the dashboard API are separate processes that share PostgreSQL and
nothing else.

The alternative — one process with a gateway connection *and* an HTTP server — is
simpler to deploy and worse to operate. The bot holds a websocket that must stay up
for months; the API gets redeployed whenever a route changes. Coupling them means
every dashboard deploy blips the bot's presence, and a panic while serving a request
takes the gateway down with it.

The cost is that the two can disagree about state for as long as a transaction takes,
which is why every shared rule lives in `domain/` rather than in either binary. A
leveling curve, a permission check, an XP cooldown: one implementation, two callers.

## The four layers

```
presentation ──▶ application ──▶ domain
infrastructure ─▶ domain          (implements the traits domain declares)
presentation ──▶ infrastructure   (only to assemble AppState)
```

The rule that matters is that **`domain` depends on nothing**. No `axum::http`, no
`sea_orm`, no `serenity`. It declares what the business needs — entities, repository
traits, error types — and the outer layers satisfy those declarations.

That is not architecture for its own sake. It is what lets a slash command and an
HTTP handler apply the same rule without either one owning it. When
`GrantMessageXpUseCase` applies a cooldown, the bot and the dashboard cannot drift
apart, because there is one cooldown implementation and it lives behind a trait.

Concretely, what that buys:

| Question | Where it is answered |
|---|---|
| May this user change that member's role? | `domain/guild_member/errors.rs` |
| Is this XP amount acceptable? | `domain/leveling/errors.rs` |
| Has this member claimed their daily? | `domain/economy/entity.rs` |
| Which HTTP status is that? | `presentation/error.rs`, and nowhere else |

## Use cases are constructed per request

A handler builds the use case it needs from `AppState`:

```rust
let use_case = CreateUserUseCase::new(state.password_service.clone(), state.users.clone());
```

Both halves are cheap: the services are reference-counted, so this copies a couple of
pointers. Storing pre-built use cases in `AppState` would work and would grow the
state by a field per feature, and would hide each handler's dependencies behind a
shared bag. Building them inline makes a handler's dependencies legible in its own
body.

## `AppState` holds concrete types, not trait objects

`Box<dyn UserRepository>` would allow swapping implementations at runtime, which
nothing needs. What it would cost is a vtable indirection on every call and, more
importantly, a layer of indirection between a reader and the code that runs. The
use cases are generic over the traits, so a test can substitute a fake; production
names the one implementation that exists.

## Trait methods use `impl Future`, not `#[async_trait]`

Edition 2024 makes `async fn` in traits work, and `impl Future` in the signature
avoids the boxing `#[async_trait]` performs. The cost is that the repository traits
are not object-safe — they cannot be used as `dyn UserRepository` — which is the
thing `AppState` deliberately does not want anyway.

## Errors: three translations, one decision

```
DbErr ──▶ RepositoryError ──▶ AppError ──▶ HTTP status
         (infrastructure)     (presentation)
```

Each arrow is a single function, so "what status does a conflict produce?" has one
answer. Adding a domain error means adding one `From` arm; the compiler finds every
place that needs updating.

`AppError::Internal` never puts its detail in the response. The detail is logged with
the tracing span, and the client gets a generic message — a 500 that leaks a SQL
fragment is how a schema becomes public.

## Snowflakes are strings

Discord ids are 64-bit integers larger than 2^53. A JavaScript client that parses one
as a number rounds it silently, producing a plausible-looking id that belongs to
nobody. So they cross the wire as strings, and `presentation/validation.rs` parses
them with a message that says so — "must be a discord id (digits only)" rather than
"invalid", because the failure is almost always a client that JSON-parsed the number
somewhere upstream.

## Authorization is two-layer

- **Platform** (`users.is_super_admin`): operate the installation. Bypasses guild
  checks entirely — the escape hatch that keeps a broken permission matrix from
  bricking a guild permanently.
- **Guild** (`guild_members` → `guild_roles` → `guild_role_permissions`): what this
  person may do in *this* server. Roles are rows, not an enum, so the matrix is
  editable from the dashboard without a deploy.

Every guild-scoped handler opens with:

```rust
authorize(&state, actor.id, guild_id, "economy:write").await?;
```

A function rather than a middleware, because the check needs the guild id from the
path — a layer would re-parse what the handler is about to parse anyway, and could not
skip the routes that are not guild-scoped. Putting it first in the body also means a
reader sees the authorization requirement before the logic.

## Guards that exist because someone got it wrong once

- **A guild must keep a role granting `dashboard:manage`.** Editing the matrix to
  remove your own last capability locks the guild permanently; the edit is refused
  instead.
- **A role with members cannot be deleted.** The foreign key is `ON DELETE RESTRICT`,
  so the database would refuse anyway — failing in the use case produces a readable
  message instead of a 500.
- **Nobody changes their own membership role.** The most common way an admin
  accidentally locks themselves out mid-session.
- **The guild owner cannot be demoted** through the dashboard.
- **A welcome template is validated when it is saved**, not when a member joins. A
  typo like `{membercount}` would otherwise post literally forever and nobody would
  notice.

## Money moves in one place

`EconomyRepository::apply_transaction` is the only write path for a balance. It opens
a transaction, re-reads the balance *inside* it, inserts the ledger row and updates
the cached balance, then commits.

The balance column is a cache of the append-only ledger, kept so "show my balance" is
one row read instead of a scan. Computing the new balance from a value read outside
the transaction is exactly how currency gets duplicated under two concurrent
purchases, which is why the read is inside it.

Purchases check stock *before* taking money: `decrement_stock_if_available` is a
single guarded `UPDATE`, and zero rows affected means someone else bought the last
one — a lost race, reported as such, with no ledger entry written.

## Delayed autoroles are rows, not timers

An in-memory timer per pending grant would be simpler and would lose the queue on
restart. For an anti-raid feature — grant the role to members who are still here in
ten minutes — losing the queue is losing the feature.

So a grant is a row with a `due_at`, and a sweeper polls every minute. A bot restart
resumes the queue. A member who left is detected when the grant comes due and the row
is dropped rather than retried forever; a role the bot cannot grant (positioned above
its own highest role) backs off and eventually gives up, logged once instead of every
minute.

## Known gaps

These are open, documented, and each one names what closing it would take. None is
silent.

- **`GuildMemberRepository` cannot answer "is this user the guild owner?".** It
  exposes no Discord id for a member, and `UserRepository` only goes
  discord-id → user, never the reverse. So the owner half of
  `ensure_can_change_membership` is not evaluated when a member's dashboard role is
  changed; the self-edit guard and the last-`dashboard:manage`-holder guard both
  are. Closing it needs one of: a `user_id → discord_id` lookup, or a
  `UserRepository` parameter on the two membership use cases.
- **`economy_purchases` has no writer.** A purchase writes a ledger entry and
  decrements stock, but never records the purchase itself, because
  `NewTransaction` carries no item reference and the repository is the only writer
  of that table. The ledger is authoritative for "what did this member spend", so
  nothing is lost; a per-item purchase history needs an item field on
  `NewTransaction`.
- **`/guilds/{id}/roles` reports `member_count: 0`.** `GuildRoleRepository` has no
  per-role member count, so the use case takes an optional count port that the
  handlers do not supply. The count is exact on the delete and matrix paths, which
  take a member repository. Closing it is one line per handler:
  `ListGuildRolesUseCase::with_counts(roles, MemberRoleCounts::new(members))`.
- **Reward roles are only reconciled on a level change.** A reward added *after*
  members have already passed its level is not granted retroactively; that needs a
  reconciliation pass over the profiles above the new threshold.
- **A failed role grant after a purchase is not rolled back.** The ledger has
  already moved, so the reply says the purchase succeeded and the role could not be
  given. An admin grants it by hand; an automatic retry would need the pending-grant
  queue the autoroles already have.

## What was left out, and where it would go

- **OAuth pending states are in process memory.** Correct for one replica, and it
  means a stolen database row cannot be replayed as a login. A second replica needs
  Redis; `docker-compose.dev.yml` already runs one.
- **No rate limiting on the API.** A reverse proxy in front of the gateway is the
  right place for it; the guard that matters today is the permission matrix.
- **`/me` resolves the caller's role and permissions per guild in a loop.** For a
  user in ten guilds that is ten queries. The fix is one joined query; the reason it
  has not been done is that the fan-out is small and the repository method it would
  need (`permissions_for_member`) is the one the authorization hot path uses, so
  changing it touches everything.
- **Refresh tokens are hashed, not encrypted.** SHA-256 rather than Argon2: the token
  is already high-entropy random, so a slow hash would cost every refresh a hundred
  milliseconds of CPU for no added security.
