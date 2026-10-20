import { queryOptions } from "@tanstack/react-query"
import {
	autorolesApi,
	economyApi,
	guildsApi,
	jailApi,
	levelingApi,
	membersApi,
	panelsApi,
	rolesApi,
	welcomeApi,
} from "#/libs/api/resources.ts"
import type { IListParams } from "#/libs/api/types.ts"
import { queryKeys } from "#/libs/query/keys.ts"

/**
 * Query option factories for every guild-scoped resource.
 *
 * Centralised for three reasons:
 *  - the key and the fetcher can never drift apart, which is the usual cause of
 *    a mutation that invalidates nothing;
 *  - `useQuery(guildQueries.members(id, params))` in a component and
 *    `queryClient.ensureQueryData(guildQueries.members(id, params))` in a loader
 *    are provably the same cache entry;
 *  - the default `staleTime` per resource is declared once, so a config object
 *    that other people edit is re-read promptly while a leaderboard is not.
 */

/** Configuration that admins edit: refetch eagerly after invalidation. */
const CONFIG_STALE_TIME = 30_000
/** Derived rankings: short-lived, cheap to recompute. */
const LEADERBOARD_STALE_TIME = 15_000
/** Paginated tables: the user is driving, so trust the cache. */
const TABLE_STALE_TIME = 30_000

export const guildQueries = {
	detail: (guildId: string) =>
		queryOptions({
			queryKey: queryKeys.guilds.detail(guildId),
			queryFn: ({ signal }) => guildsApi.get(guildId, { signal }),
			staleTime: CONFIG_STALE_TIME,
		}),

	/**
	 * Role and channel names for every picker in this guild.
	 *
	 * `Infinity` rather than a timer: the list only changes when someone edits
	 * roles in Discord, and re-reading it on every config page would spend a
	 * Discord round trip per navigation to answer a question whose answer did not
	 * move. The dashboard's own "Refresh names" control invalidates it explicitly.
	 */
	directory: (guildId: string) =>
		queryOptions({
			queryKey: queryKeys.guilds.directory(guildId),
			queryFn: ({ signal }) => guildsApi.directory(guildId, { signal }),
			staleTime: Number.POSITIVE_INFINITY,
			// A deployment with no bot token cannot answer this, and a retry loop
			// would just repeat a 503 that will never become a 200.
			retry: false,
		}),

	/* ---------------------------------------------------------------- members */

	members: (guildId: string, params: IListParams) =>
		queryOptions({
			queryKey: queryKeys.members.list(guildId, params),
			queryFn: ({ signal }) => membersApi.list(guildId, params, { signal }),
			staleTime: TABLE_STALE_TIME,
			placeholderData: (previous) => previous,
		}),

	/* ------------------------------------------------------------------ roles */

	roles: (guildId: string, params: IListParams = { per_page: 100 }) =>
		queryOptions({
			queryKey: queryKeys.roles.list(guildId, params),
			queryFn: ({ signal }) => rolesApi.list(guildId, params, { signal }),
			staleTime: CONFIG_STALE_TIME,
		}),

	permissions: (guildId: string) =>
		queryOptions({
			queryKey: queryKeys.roles.permissions(guildId),
			queryFn: ({ signal }) => rolesApi.permissions(guildId, { signal }),
			// The permission catalogue is compiled into the server, so it never
			// changes while the tab is open.
			staleTime: Number.POSITIVE_INFINITY,
		}),

	/* ---------------------------------------------------------------- economy */

	economyLeaderboard: (guildId: string, limit = 10) =>
		queryOptions({
			queryKey: queryKeys.economy.leaderboard(guildId, limit),
			queryFn: ({ signal }) => economyApi.leaderboard(guildId, limit, { signal }),
			staleTime: LEADERBOARD_STALE_TIME,
		}),

	economyAccount: (guildId: string, discordId: string, enabled = true) =>
		queryOptions({
			queryKey: queryKeys.economy.account(guildId, discordId),
			queryFn: ({ signal }) => economyApi.account(guildId, discordId, { signal }),
			staleTime: CONFIG_STALE_TIME,
			enabled: enabled && discordId.length > 0,
			// A missing account is a normal answer (nobody has earned yet), not a
			// fault worth retrying or exploding over.
			retry: false,
		}),

	transactions: (guildId: string, discordId: string, params: IListParams, enabled = true) =>
		queryOptions({
			queryKey: queryKeys.economy.transactions(guildId, discordId, params),
			queryFn: ({ signal }) => economyApi.transactions(guildId, discordId, params, { signal }),
			staleTime: TABLE_STALE_TIME,
			enabled: enabled && discordId.length > 0,
			placeholderData: (previous) => previous,
		}),

	items: (guildId: string, params: IListParams) =>
		queryOptions({
			queryKey: queryKeys.economy.items(guildId, params),
			queryFn: ({ signal }) => economyApi.items(guildId, params, { signal }),
			staleTime: CONFIG_STALE_TIME,
			placeholderData: (previous) => previous,
		}),

	/* --------------------------------------------------------------- leveling */

	levelingConfig: (guildId: string) =>
		queryOptions({
			queryKey: queryKeys.leveling.config(guildId),
			queryFn: ({ signal }) => levelingApi.config(guildId, { signal }),
			staleTime: CONFIG_STALE_TIME,
		}),

	levelingLeaderboard: (guildId: string, limit = 25) =>
		queryOptions({
			queryKey: queryKeys.leveling.leaderboard(guildId, limit),
			queryFn: ({ signal }) => levelingApi.leaderboard(guildId, limit, { signal }),
			staleTime: LEADERBOARD_STALE_TIME,
		}),

	rewards: (guildId: string, params: IListParams) =>
		queryOptions({
			queryKey: queryKeys.leveling.rewards(guildId, params),
			queryFn: ({ signal }) => levelingApi.rewards(guildId, params, { signal }),
			staleTime: CONFIG_STALE_TIME,
			placeholderData: (previous) => previous,
		}),

	/* ---------------------------------------------------------------- welcome */

	welcome: (guildId: string) =>
		queryOptions({
			queryKey: queryKeys.welcome.config(guildId),
			queryFn: ({ signal }) => welcomeApi.get(guildId, { signal }),
			staleTime: CONFIG_STALE_TIME,
		}),

	/**
	 * The rendered join message, for the preview pane.
	 *
	 * Short-lived: it is derived from the saved config, so a save invalidates it and
	 * a stale preview is worse than a spinner.
	 */
	welcomePreview: (guildId: string) =>
		queryOptions({
			queryKey: queryKeys.welcome.preview(guildId),
			queryFn: ({ signal }) => welcomeApi.preview(guildId, { signal }),
			staleTime: LEADERBOARD_STALE_TIME,
		}),

	/* ----------------------------------------------------------------- panels */

	panels: (guildId: string, params: IListParams) =>
		queryOptions({
			queryKey: queryKeys.panels.list(guildId, params),
			queryFn: ({ signal }) => panelsApi.list(guildId, params, { signal }),
			staleTime: CONFIG_STALE_TIME,
			placeholderData: (previous) => previous,
		}),

	/**
	 * One panel with its buttons.
	 *
	 * The list returns each panel's option *count*, not the options themselves, so
	 * opening a panel is what fetches its buttons. Same split the API makes.
	 */
	panel: (guildId: string, panelId: string) =>
		queryOptions({
			queryKey: queryKeys.panels.detail(guildId, panelId),
			queryFn: ({ signal }) => panelsApi.get(guildId, panelId, { signal }),
			staleTime: CONFIG_STALE_TIME,
		}),

	/* ------------------------------------------------------------------- jail */

	jailConfig: (guildId: string) =>
		queryOptions({
			queryKey: queryKeys.jail.config(guildId),
			queryFn: ({ signal }) => jailApi.config(guildId, { signal }),
			staleTime: CONFIG_STALE_TIME,
		}),

	jailSentences: (guildId: string) =>
		queryOptions({
			queryKey: queryKeys.jail.sentences(guildId),
			queryFn: ({ signal }) => jailApi.sentences(guildId, { signal }),
			// Sentences change on the server's sweeper, not because of anything this
			// page did, so a short stale time keeps "2m left" honest without polling
			// hard enough to matter.
			staleTime: 15_000,
		}),

	/* -------------------------------------------------------------- autoroles */

	autoroles: (guildId: string, params: IListParams) =>
		queryOptions({
			queryKey: queryKeys.autoroles.list(guildId, params),
			queryFn: ({ signal }) => autorolesApi.list(guildId, params, { signal }),
			staleTime: CONFIG_STALE_TIME,
			placeholderData: (previous) => previous,
		}),
}
