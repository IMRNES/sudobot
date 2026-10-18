# web — sudobot dashboard

The React single-page dashboard for the sudobot Discord bot. It talks to the
Rust Axum service over the HTTP API documented in [`../API.md`](../API.md) and
holds no server state of its own.

## Running it

From the repository root (this is a pnpm workspace, and `web` is the only
package in it):

```sh
pnpm install                     # installs the workspace
pnpm --filter web dev            # dev server on http://localhost:5173
pnpm --filter web build          # production bundle into web/dist
pnpm --filter web preview        # serve the built bundle
```

Via moon, from the repository root:

```sh
moon run web:dev                 # dev server
moon run web:build               # production build
moon run web:check               # Biome check (lint + format)
moon run web:typecheck           # tsc --noEmit
moon run web:lint                # Biome lint only
```

moon pins the toolchain through [`.moon/toolchains.yml`](../.moon/toolchains.yml)
(node + pnpm), so `moon run web:build` behaves the same on every machine.

## Where the API base URL comes from

`VITE_API_BASE_URL`, read at build time in `src/libs/api/client.ts`:

```sh
cp web/.env.example web/.env.local   # then edit
```

- **Local development** — the Axum service listens on `:8080` and Vite on
  `:5173`, so set `VITE_API_BASE_URL=http://localhost:8080`.
- **Deployed** — leave it **empty**. The client then issues same-origin requests,
  which is what a reverse proxy in front of both the SPA and `/api` wants.

The client appends the API's own prefix (`/api/v1/iam`) itself, so the value is
an origin only — never include `/api` in it.

The service must allow this origin via `CORS_ORIGINS` (`WEB_ORIGIN` in
`../.env.example`); otherwise every request fails with the network-error copy.

## Golden path

1. Open `/`. Signed out, you get the landing page; signed in, the loader bounces
   you straight to `/dashboard`.
2. **Sign in with Discord** → `POST /auth/discord/authorize` returns
   `{authorize_url, state}`. The `state` nonce is written to `sessionStorage` and
   the browser is sent to Discord.
3. Discord redirects back to `/auth/callback?code=…&state=…`. The route compares
   `state` against the stored nonce, then calls `POST /auth/discord/callback`,
   stores the token pair, seeds the session cache from the response, and routes
   to `/dashboard`. A mismatched `state` is refused without exchanging the code.
4. `/dashboard` lists the servers you can manage. Opening one lands on
   `/{guildId}`, the guild overview.
5. The sidebar's guild switcher moves between servers; the nav below it lists
   only the sections your dashboard role can read.
6. Feature pages: **Economy** (leaderboard, account lookup + ledger, shop item
   CRUD), **Leveling** (XP config, level rewards, leaderboard), **Members**
   (search + dashboard-role assignment), **Roles** (permission matrix, role
   CRUD), **Welcome** (message templates with placeholder validation),
   **Autoroles** (trigger rules), **Settings** (server name).

## Architecture notes

**Layouts are file + sibling folder pairs.** `_authenticated.tsx` is the guard
and `_authenticated/` holds the children it renders; the same for
`$guildSlug.tsx` + `$guildSlug/`. Both halves are required — the file alone
renders no children, the folder alone has no guard.

**Underscore folders are invisible to the router.** `_components/`, `_data/` and
`_hooks/` sit at whichever layout level owns them, and
`routeFileIgnorePattern` in `vite.config.ts` keeps them out of the route tree.

**`$guildSlug` is the guild id.** The layout resolves it against the session's
guild list; an id that is not one of yours is a 404. That lookup is also what
publishes `role_slug` and `permissions` into router context.

**Permission gating.** `useCan("economy:write")` reads the guild's permission
list. Controls the member cannot use are rendered **disabled with a tooltip
explaining why**, never hidden, so a read-only member can still see what exists.
Nav entries for sections they cannot even read are hidden.

**Session.** The access token is held in memory and the refresh token in
`localStorage`. On a `401` the client refreshes **once** — concurrent 401s share
a single refresh promise, because the API rotates refresh tokens — and replays
the original request; a failed refresh clears the session and returns the user to
`/`. See the note at the top of `src/libs/auth/session.ts`: a production
deployment should use an `HttpOnly` cookie instead of `localStorage`.

**Lists.** Pagination follows the API contract exactly (`?page=&per_page=&search=`,
plus `sort_by`/`sort_direction`), reads `meta` from the response body and falls
back to the `X-Total-Count` family of headers. Table state lives in the route's
search params, so a filtered view is shareable. Every list renders through the
shared `DataTable` + `Pagination` composites.

**Query keys** come from factories in `src/libs/query/keys.ts` — one namespace
per feature, never an inline array literal, so an invalidation can never miss.

**Error handling** normalises every failure into one shape via
`src/libs/errors/`, so components render `{"error": "..."}` from the server
directly rather than branching on transport details.

## Layout

```
web/
  src/
    libs/            api client, auth/session, query client + keys, errors, utils
    components/
      ui/            shadcn primitives
      layout/        app shell, sidebar, guild switcher, route states
      features/      shared composites: DataTable, Pagination, permission gates…
    hooks/           useCan, useMe, table search/pagination
    routes/          see "Layouts are file + sibling folder pairs" above
```

`src/routeTree.gen.ts` is generated by `@tanstack/router-plugin` on every dev
start and build — never edit it, and keep it in Biome's ignore list.

## Conventions

- TypeScript strict; relative imports carry explicit `.ts`/`.tsx` extensions.
- `T` prefixes types, `I` interfaces, `E` enums; kebab-case for every file and
  folder name.
- Biome: tabs, double quotes, semicolons only where required.
- Path alias `#/*` maps to `./src/*`.
