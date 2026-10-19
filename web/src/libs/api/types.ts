/**
 * Hand-written mirrors of the JSON shapes documented in `/API.md`.
 *
 * Conventions that matter when reading these types:
 *  - Discord snowflakes are **strings**. They exceed `Number.MAX_SAFE_INTEGER`,
 *    so parsing them as numbers would silently corrupt them.
 *  - Timestamps are RFC 3339 strings; format them at the edge, never store `Date`s.
 *  - Optional-but-present fields (`avatar_url`, `last_daily_at`, ...) are `T | null`
 *    rather than `?` so a missing key and an explicit `null` are the same thing.
 */

/* -------------------------------------------------------------------------- */
/* Envelopes                                                                   */
/* -------------------------------------------------------------------------- */

export type TSortDirection = "asc" | "desc"

/** Pagination block returned in the body of every list route. */
export interface IPaginationMeta {
	page: number
	per_page: number
	total: number
	total_pages: number
	has_next: boolean
	has_prev: boolean
}

/** Envelope of every list route: `{ data, meta }`. */
export interface TListResponse<TItem> {
	data: TItem[]
	meta: IPaginationMeta
}

/** Query parameters accepted by every list route. */
export interface IListParams {
	page?: number
	per_page?: number
	sort_by?: string
	sort_direction?: TSortDirection
	search?: string
	/** Repeatable `<field>:<op>:<value>` filters. */
	filter?: string[]
}

/** Shape of every error body the API emits. */
export interface TApiErrorBody {
	error: string
	code?: string
}

/* -------------------------------------------------------------------------- */
/* Auth                                                                        */
/* -------------------------------------------------------------------------- */

export interface IUser {
	id: string
	discord_id: string
	username: string
	global_name: string | null
	avatar_url: string | null
	is_super_admin: boolean
}

export interface IGuildSummary {
	id: string
	name: string
	icon_url: string | null
	member_count: number
	bot_present: boolean
	role_slug: string
	role_name: string
	permissions: string[]
}

export interface IGuildDetail extends IGuildSummary {
	created_at: string
	updated_at: string
	owner_discord_id: string
}

export interface IAuthorizeResponse {
	authorize_url: string
	state: string
}

export interface ICallbackRequest {
	code: string
	state: string
}

export interface IAuthTokens {
	access_token: string
	refresh_token: string
	expires_in: number
}

export interface ICallbackResponse extends IAuthTokens {
	user: IUser
	/**
	 * The single guild this dashboard serves.
	 *
	 * Populated by the callback itself: the guild is configuration
	 * (`DASHBOARD_GUILD_ID`), and the caller already proved they hold a
	 * whitelisted role there, so there is nothing left to import. A signed-in
	 * session therefore never renders an empty server list.
	 */
	guilds: IGuildSummary[]
}

export interface IMeResponse {
	user: IUser
	guilds: IGuildSummary[]
}

export interface IRefreshResponse extends IAuthTokens {}

export interface ILogoutResponse {
	revoked: number
}

/* -------------------------------------------------------------------------- */
/* Members                                                                     */
/* -------------------------------------------------------------------------- */

export interface IMemberResponse {
	user_id: string
	discord_id: string
	username: string
	global_name: string | null
	avatar_url: string | null
	guild_role_id: string | null
	role_slug: string | null
	role_name: string | null
	discord_permissions: string
	is_owner: boolean
	synced_at: string
}

export interface IMemberUpdateRequest {
	guild_role_id: string
}

/* -------------------------------------------------------------------------- */
/* Roles and permissions                                                       */
/* -------------------------------------------------------------------------- */

export interface IPermissionResponse {
	id: string
	/** Dotted key such as `economy:read` — what `useCan()` checks against. */
	name: string
	group: string
	description: string
}

export interface IRoleResponse {
	id: string
	slug: string
	name: string
	description: string | null
	is_system: boolean
	position: number
	member_count: number
	permissions: string[]
}

export interface IRoleCreateRequest {
	slug: string
	name: string
	description?: string | null
	position?: number
}

export interface IRoleUpdateRequest {
	name?: string
	description?: string | null
	position?: number
}

export interface IRolePermissionsUpdateRequest {
	permissions: string[]
}

/* -------------------------------------------------------------------------- */
/* Economy                                                                     */
/* -------------------------------------------------------------------------- */

export interface IAccountResponse {
	discord_id: string
	balance: number
	total_earned: number
	total_spent: number
	last_daily_at: string | null
}

export interface ITransactionResponse {
	id: string
	amount: number
	balance_after: number
	reason: string
	/** The admin's own words on an adjustment; absent on machine-written rows. */
	note?: string | null
	actor_discord_id: string | null
	created_at: string
}

export interface IAccountAdjustRequest {
	/** Positive grants, negative deducts. */
	amount: number
	reason: string
}

/**
 * Economy leaderboard row.
 *
 * API.md documents the leveling leaderboard's shape but not this one, so the
 * fields here are inferred from the documented `AccountResponse` and from the
 * leveling entry, and cover the superset a ranked list would plausibly return.
 * `balance` is the only field rendered; the rest are optional so a leaner
 * server response still type-checks.
 */
export interface IEconomyLeaderboardEntryResponse {
	rank: number
	discord_id: string
	balance: number
	level?: number
	total_earned?: number
	total_spent?: number
}

export interface IShopItemResponse {
	id: string
	slug: string
	name: string
	description: string | null
	price: number
	role_id: string | null
	/** `null` means unlimited stock. */
	stock: number | null
	is_active: boolean
}

export interface IShopItemCreateRequest {
	slug: string
	name: string
	description?: string | null
	price: number
	role_id?: string | null
	stock?: number | null
}

export type TShopItemUpdateRequest = Partial<IShopItemCreateRequest> & {
	is_active?: boolean
}

/* -------------------------------------------------------------------------- */
/* Leveling                                                                    */
/* -------------------------------------------------------------------------- */

export type TLevelingCurve = "mrn" | "linear" | "quadratic" | "exponential" | (string & {})

export interface ILevelingConfigResponse {
	enabled: boolean
	xp_min: number
	xp_max: number
	cooldown_secs: number
	voice_xp_multiplier: number
	curve: TLevelingCurve
	announce_channel_id: string | null
	stack_roles: boolean
	ignored_channel_ids: string[]
	ignored_role_ids: string[]
}

export type TLevelingConfigUpdateRequest = Partial<ILevelingConfigResponse>

export interface ILeaderboardEntryResponse {
	rank: number
	discord_id: string
	level: number
	xp: number
	total_xp: number
	message_count: number
	/** 0..1 progress towards the next level. */
	progress: number
}

export interface IRewardResponse {
	id: string
	level: number
	role_id: string
	keep_on_demote: boolean
}

export interface IRewardCreateRequest {
	level: number
	role_id: string
	keep_on_demote: boolean
}

/* -------------------------------------------------------------------------- */
/* Welcome and role panels                                                     */
/* -------------------------------------------------------------------------- */

export interface IWelcomeConfigResponse {
	welcome_enabled: boolean
	welcome_channel_id: string | null
	welcome_message: string | null
	welcome_embed_color: number | null
	welcome_dm_enabled: boolean
	goodbye_enabled: boolean
	goodbye_channel_id: string | null
	goodbye_message: string | null
	goodbye_embed_color: number | null
}

export type TWelcomeConfigUpdateRequest = Partial<IWelcomeConfigResponse>

/** Placeholders the API accepts inside a welcome or goodbye message. */
export const WELCOME_PLACEHOLDERS = ["{user}", "{mention}", "{guild}", "{memberCount}"] as const

export type TPanelOptionBehavior = "toggle" | "unique" | (string & {})

export interface IPanelOptionResponse {
	id: string
	label: string
	emoji: string | null
	role_id: string
	/** Discord button style integer (1 primary … 4 danger). */
	style: number
	position: number
	behavior: TPanelOptionBehavior
}

export interface IPanelResponse {
	id: string
	slug: string
	title: string
	description: string | null
	channel_id: string | null
	message_id: string | null
	is_published: boolean
	options: IPanelOptionResponse[]
}

/**
 * A panel as the *list* route returns it.
 *
 * Deliberately not `IPanelResponse`: the list carries `option_count` instead of the
 * options themselves, because a guild's panels are listed to be chosen from, not
 * rendered. Fetching one is what returns its buttons.
 */
export interface IPanelSummaryResponse {
	id: string
	slug: string
	title: string
	channel_id: string | null
	is_published: boolean
	option_count: number
}

export interface IPanelPublishRequest {
	/**
	 * The channel to post the panel into.
	 *
	 * Nothing else: the API posts with the bot token and records the message id it
	 * gets back. Sending a message id was never a publish — it only recorded a
	 * message somebody else had already posted.
	 */
	channel_id: string
}

export interface IPanelCreateRequest {
	slug: string
	title: string
	description?: string | null
	channel_id?: string | null
}

export type TPanelUpdateRequest = Partial<IPanelCreateRequest>

export interface IPanelOptionCreateRequest {
	label: string
	emoji?: string | null
	role_id: string
	style: number
	position?: number
	behavior: TPanelOptionBehavior
}

/* -------------------------------------------------------------------------- */
/* Autoroles                                                                   */
/* -------------------------------------------------------------------------- */

export type TAutoRoleTrigger = "join" | "delay" | "human"

export interface IAutoRoleResponse {
	id: string
	role_id: string
	trigger: TAutoRoleTrigger
	delay_seconds: number
	applies_to_bots: boolean
	is_active: boolean
}

export interface IAutoRoleCreateRequest {
	role_id: string
	trigger: TAutoRoleTrigger
	delay_seconds: number
	applies_to_bots: boolean
}

export type TAutoRoleUpdateRequest = Partial<IAutoRoleCreateRequest> & {
	is_active?: boolean
}

/* -------------------------------------------------------------------------- */
/* Health                                                                      */
/* -------------------------------------------------------------------------- */

export interface IHealthResponse {
	status: string
	database: string
}

/* -------------------------------------------------------------------------- */
/* Client-side composites                                                      */
/* -------------------------------------------------------------------------- */

/**
 * Normalised error shape produced by `libs/errors`. Every server error reaching
 * a component has been through `normalizeApiError`, so components never branch
 * on transport details.
 */
export interface IAppError {
	message: string
	status: number | null
	code: string | null
	/** Per-field messages when the server returns a validation payload. */
	fields: Record<string, string>
}
