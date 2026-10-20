# sudobot HTTP API

Base path: `/api/v1/iam`. All request and response bodies are JSON.

## Conventions

**Auth.** `Authorization: Bearer <access_token>` on every route except
`/auth/*`. An expired access token returns `401` with
`{"error": "...", "code": "token_expired"}`; the client refreshes and retries.

**Errors.** Every error response is `{"error": "<human readable>"}` with an
optional `"code"` for cases the client must branch on. Status codes: `400`
validation, `401` unauthenticated, `403` authenticated but not allowed, `404`
missing, `409` conflict, `500` server fault.

**Pagination.** List routes accept `?page=1&per_page=20&sort_by=<field>&sort_direction=asc|desc&search=<text>&filter=<field>:<op>:<value>` (repeatable).
Responses are:

```json
{
  "data": [ ... ],
  "meta": {
    "page": 1,
    "per_page": 20,
    "total": 137,
    "total_pages": 7,
    "has_next": true,
    "has_prev": false
  }
}
```

The same values are repeated in `X-Total-Count`, `X-Total-Pages`,
`X-Current-Page` and `X-Per-Page` headers.

**Guild scoping.** Every route below `/guilds/{guildId}` requires the caller to
hold the guild permission named in the *Permission* column. A `403` means they
do not.

---

## Auth

| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/health` | — | `{"status":"ok","database":"ok"}` |
| POST | `/auth/discord/authorize` | — | `{"authorize_url":"https://discord.com/oauth2/authorize?...","state":"<hex>"}` |
| POST | `/auth/discord/callback` | `{"code":"...","state":"..."}` | `{"user": User, "access_token":"...","refresh_token":"...","expires_in":900,"guilds": GuildSummary[]}` |
| POST | `/auth/refresh` | `{"refresh_token":"..."}` | `{"access_token":"...","refresh_token":"...","expires_in":900}` |
| POST | `/auth/logout` | `{"refresh_token":"...","everywhere":false}` | `{"revoked":1}` |
| GET | `/me` | — | `{"user": User, "guilds": GuildSummary[]}` |

`User`:

```json
{
  "id": "uuid",
  "discord_id": "123456789012345678",
  "username": "name",
  "global_name": "Name",
  "avatar_url": "https://cdn.discordapp.com/...",
  "is_super_admin": false
}
```

`GuildSummary`:

```json
{
  "id": "123456789012345678",
  "name": "My Server",
  "icon_url": null,
  "member_count": 128,
  "bot_present": true,
  "role_slug": "admin",
  "role_name": "Admin",
  "permissions": ["economy:read", "economy:write", "..."]
}
```

Note: snowflakes are **strings** in JSON, because they exceed 2^53 and would
lose precision as JavaScript numbers.

---

## Guilds

| Method | Path | Permission | Notes |
|---|---|---|---|
| GET | `/guilds` | — (own memberships) | paginated `GuildSummary` |
| GET | `/guilds/{guildId}` | `members:read` | `GuildDetail` |
| PATCH | `/guilds/{guildId}` | `dashboard:manage` | `{"name":"..."}` |
| GET | `/guilds/{guildId}/directory` | any feature `:read` | role and channel names |

`GuildDetail` = `GuildSummary` plus
`{"created_at": "...", "updated_at": "...", "owner_discord_id": "..."}`.

`GuildDirectory` is how the dashboard renders a name instead of a snowflake.
Every configuration screen stores Discord ids, and an id tells a human nothing, so
each picker resolves through this. It is read with the **bot** token — no visitor
has authority to enumerate a guild's roles — and the API keeps no copy of the
names, so a role renamed in Discord shows its new name on the next load.

```json
{
  "roles": [
    {"id": "1460146703637287115", "name": "Neophyte", "color": 5793266,
     "position": 22, "hoist": true, "mentionable": false, "managed": false}
  ],
  "channels": [{"id": "1561374062926630912", "name": "general", "kind": 0, "parent_id": null}]
}
```

Roles come back highest-position first, channels in name order. `managed` marks a
bot or integration role, which cannot be assigned by hand — the pickers label those
so an admin is not offered a choice that silently does nothing. `kind` is Discord's
raw channel type (`0` text, `2` voice, `4` category, `5` news, `13` stage,
`15` forum).

Gated on holding **any** one of `leveling:read`, `economy:read`, `welcome:read`,
`autorole:read`, `roles:read`, `members:read` rather than a single permission: this
is shared reference data, and gating it on one feature's read permission would 404 a
page whose picker needed it. A `503` means the installation has no bot token, and
the dashboard degrades to showing raw ids.

---

## Members

| Method | Path | Permission |
|---|---|---|
| GET | `/guilds/{guildId}/members` | `members:read` |
| PATCH | `/guilds/{guildId}/members/{userId}` | `members:write` |
| DELETE | `/guilds/{guildId}/members/{userId}` | `members:write` |

`PATCH` body: `{"guild_role_id": "uuid"}`. Response `MemberResponse`:

```json
{
  "user_id": "uuid",
  "discord_id": "123456789012345678",
  "username": "name",
  "global_name": "Name",
  "avatar_url": null,
  "guild_role_id": "uuid",
  "role_slug": "moderator",
  "role_name": "Moderator",
  "discord_permissions": "8",
  "is_owner": false,
  "synced_at": "2026-10-18T12:00:00Z"
}
```

---

## Roles and permissions

| Method | Path | Permission |
|---|---|---|
| GET | `/guilds/{guildId}/permissions` | `roles:read` |
| GET | `/guilds/{guildId}/roles` | `roles:read` |
| POST | `/guilds/{guildId}/roles` | `roles:write` |
| PATCH | `/guilds/{guildId}/roles/{roleId}` | `roles:write` |
| DELETE | `/guilds/{guildId}/roles/{roleId}` | `roles:write` |
| PUT | `/guilds/{guildId}/roles/{roleId}/permissions` | `roles:write` |

`PermissionResponse`: `{"id":"uuid","name":"economy:read","group":"economy","description":"..."}`

`RoleResponse`:

```json
{
  "id": "uuid",
  "slug": "moderator",
  "name": "Moderator",
  "description": "Day-to-day management",
  "is_system": true,
  "position": 10,
  "member_count": 4,
  "permissions": ["members:read", "members:write"]
}
```

`POST` body: `{"slug":"events","name":"Events","description":"...","position":20}`
`PATCH` body: `{"name":"...","description":"...","position":20}`
`PUT .../permissions` body: `{"permissions":["members:read","economy:read"]}` —
replaces the whole matrix. Returns the updated `RoleResponse`.

---

## Economy

| Method | Path | Permission |
|---|---|---|
| GET | `/guilds/{guildId}/economy/leaderboard?limit=10` | `economy:read` |
| GET | `/guilds/{guildId}/economy/accounts/{discordId}` | `economy:read` |
| GET | `/guilds/{guildId}/economy/accounts/{discordId}/transactions` | `economy:read` |
| POST | `/guilds/{guildId}/economy/accounts/{discordId}/adjust` | `economy:write` |
| GET | `/guilds/{guildId}/economy/items` | `economy:read` |
| POST | `/guilds/{guildId}/economy/items` | `economy:write` |
| PATCH | `/guilds/{guildId}/economy/items/{itemId}` | `economy:write` |
| DELETE | `/guilds/{guildId}/economy/items/{itemId}` | `economy:write` |

`AccountResponse`:
`{"discord_id":"...","balance":1250,"total_earned":2000,"total_spent":750,"last_daily_at":null}`

`TransactionResponse`:
`{"id":"uuid","amount":-250,"balance_after":1250,"reason":"purchase","actor_discord_id":null,"created_at":"..."}`

`POST .../adjust` body: `{"amount":500,"reason":"event prize"}` — positive grants,
negative deducts. Returns the updated `AccountResponse`.

`ShopItemResponse`:
`{"id":"uuid","slug":"vip","name":"VIP","description":null,"price":5000,"role_id":"...","stock":null,"is_active":true}`
`POST` body: `{"slug":"vip","name":"VIP","description":"...","price":5000,"role_id":"...","stock":null}`

---

## Leveling

| Method | Path | Permission |
|---|---|---|
| GET | `/guilds/{guildId}/leveling/config` | `leveling:read` |
| PATCH | `/guilds/{guildId}/leveling/config` | `leveling:write` |
| GET | `/guilds/{guildId}/leveling/leaderboard` | `leveling:read` |
| GET | `/guilds/{guildId}/leveling/rewards` | `leveling:read` |
| POST | `/guilds/{guildId}/leveling/rewards` | `leveling:write` |
| DELETE | `/guilds/{guildId}/leveling/rewards/{rewardId}` | `leveling:write` |

`LevelingConfigResponse`:

```json
{
  "enabled": true,
  "xp_min": 15,
  "xp_max": 25,
  "cooldown_secs": 60,
  "voice_xp_multiplier": 1.0,
  "curve": "mrn",
  "announce_channel_id": null,
  "stack_roles": true,
  "ignored_channel_ids": [],
  "ignored_role_ids": []
}
```

`LeaderboardEntryResponse`:
`{"rank":1,"discord_id":"...","level":12,"xp":340,"total_xp":15200,"message_count":900,"progress":0.62}`

`RewardResponse`: `{"id":"uuid","level":5,"role_id":"...","keep_on_demote":true}`

---

## Welcome and role panels

| Method | Path | Permission |
|---|---|---|
| GET | `/guilds/{guildId}/welcome` | `welcome:read` |
| PATCH | `/guilds/{guildId}/welcome` | `welcome:write` |
| GET | `/guilds/{guildId}/panels` | `autorole:read` |
| POST | `/guilds/{guildId}/panels` | `autorole:write` |
| GET | `/guilds/{guildId}/panels/{panelId}` | `autorole:read` |
| PATCH | `/guilds/{guildId}/panels/{panelId}` | `autorole:write` |
| POST | `/guilds/{guildId}/panels/{panelId}/options` | `autorole:write` |
| DELETE | `/guilds/{guildId}/panels/{panelId}/options/{optionId}` | `autorole:write` |
| POST | `/guilds/{guildId}/panels/{panelId}/publish` | `autorole:write` |

`WelcomeConfigResponse`:

```json
{
  "welcome_enabled": true,
  "welcome_channel_id": "123",
  "welcome_message": "Welcome {mention} to {guild}!",
  "welcome_embed_color": 5793266,
  "welcome_dm_enabled": false,
  "goodbye_enabled": false,
  "goodbye_channel_id": null,
  "goodbye_message": null,
  "goodbye_embed_color": null
}
```

Placeholders allowed in a message: `{user}`, `{mention}`, `{guild}`,
`{memberCount}`. Anything else is a `400`.

`PanelResponse`:

```json
{
  "id": "uuid",
  "slug": "self-roles",
  "title": "Pick your roles",
  "description": null,
  "channel_id": "123",
  "message_id": null,
  "is_published": false,
  "options": [
    {
      "id": "uuid",
      "label": "Announcements",
      "emoji": "📢",
      "role_id": "456",
      "style": 2,
      "position": 0,
      "behavior": "toggle"
    }
  ]
}
```

A panel holds at most 25 options; exceeding that is a `400`.

---

## Autoroles

| Method | Path | Permission |
|---|---|---|
| GET | `/guilds/{guildId}/autoroles` | `autorole:read` |
| POST | `/guilds/{guildId}/autoroles` | `autorole:write` |
| PATCH | `/guilds/{guildId}/autoroles/{id}` | `autorole:write` |
| DELETE | `/guilds/{guildId}/autoroles/{id}` | `autorole:write` |

`AutoRoleResponse`:

```json
{
  "id": "uuid",
  "role_id": "456",
  "trigger": "delay",
  "delay_seconds": 600,
  "applies_to_bots": false,
  "is_active": true
}
```

`POST` body: `{"role_id":"456","trigger":"delay","delay_seconds":600,"applies_to_bots":false}`
`trigger` is one of `join`, `delay`, `human`. `delay_seconds` is 0–604800.

---

## Bot commands

The gateway bot registers these. They share the use cases the API calls, so a rule
can never differ between the two surfaces.

| Command | Options | What it does |
|---|---|---|
| `/ping` | — | Reports the database round trip and the guild count |
| `/help` | `command?` | Lists commands, or explains one |
| `/serverinfo` | — | The bot's own record for this server |
| `/economy balance` | `user?` | A member's wallet |
| `/economy daily` | — | Claims the daily reward, idempotent per interaction |
| `/economy pay` | `user`, `amount` | Transfers currency to another member |
| `/economy shop` | — | Lists the server's shop |
| `/economy leaderboard` | — | Top ten balances |
| `/level` | — | The caller's XP, level and rank |
| `/leaderboard` | — | Top ten by XP |
| `/rank` | `user?` | Another member's XP card |
| `/rolepanel` | — | Lists the server's role panels |
| `/autorole` | — | Lists the server's automatic roles |

Entities (money, leveling, shops, panels, automatic roles) are configured in the
dashboard; the commands are the member-facing half of the same data.

**Button presses** on role panels are handled by the bot's gateway event handler, not
by a command. The `custom_id` on each button is `rolepanel:{panel_id}:{index}`, which
is what makes a press resolvable back to its option without a cache.

**Delayed automatic roles** are stored as rows with a due time and swept once a
minute, so a bot restart resumes the queue rather than losing it.
