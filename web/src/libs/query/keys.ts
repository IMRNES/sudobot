import type { IListParams } from "#/libs/api/types.ts"

/**
 * Query key factories — one namespace per feature.
 *
 * Every key used by `useQuery`, `useMutation` invalidation or a route loader
 * comes from here. Inline array literals are banned: a single typo silently
 * creates a second cache entry that never invalidates, which is the most common
 * source of "the list did not refresh" bugs.
 */
export const queryKeys = {
	/** Identity and guild list for the signed-in user. */
	session: {
		all: ["session"] as const,
		me: () => [...queryKeys.session.all, "me"] as const,
	},

	health: {
		all: ["health"] as const,
		status: () => [...queryKeys.health.all, "status"] as const,
	},

	guilds: {
		all: ["guilds"] as const,
		list: (params: IListParams) => [...queryKeys.guilds.all, "list", params] as const,
		detail: (guildId: string) => [...queryKeys.guilds.all, "detail", guildId] as const,
		/** Prefix used to invalidate every cached page of the list at once. */
		lists: () => [...queryKeys.guilds.all, "list"] as const,
		/**
		 * Role and channel names for this guild's pickers.
		 *
		 * Cached for the session: it changes only when an admin edits roles or
		 * channels in Discord, and a refetch on every config page would spend a
		 * Discord round trip per navigation to learn the same names.
		 */
		directory: (guildId: string) => [...queryKeys.guilds.all, "directory", guildId] as const,
	},

	members: {
		all: (guildId: string) => ["guilds", guildId, "members"] as const,
		list: (guildId: string, params: IListParams) =>
			[...queryKeys.members.all(guildId), "list", params] as const,
		lists: (guildId: string) => [...queryKeys.members.all(guildId), "list"] as const,
	},

	roles: {
		all: (guildId: string) => ["guilds", guildId, "roles"] as const,
		list: (guildId: string, params?: IListParams) =>
			[...queryKeys.roles.all(guildId), "list", params ?? {}] as const,
		permissions: (guildId: string) => [...queryKeys.roles.all(guildId), "permissions"] as const,
	},

	economy: {
		all: (guildId: string) => ["guilds", guildId, "economy"] as const,
		leaderboard: (guildId: string, limit: number) =>
			[...queryKeys.economy.all(guildId), "leaderboard", limit] as const,
		account: (guildId: string, discordId: string) =>
			[...queryKeys.economy.all(guildId), "account", discordId] as const,
		transactions: (guildId: string, discordId: string, params: IListParams) =>
			[...queryKeys.economy.all(guildId), "transactions", discordId, params] as const,
		items: (guildId: string, params: IListParams) =>
			[...queryKeys.economy.all(guildId), "items", params] as const,
		itemLists: (guildId: string) => [...queryKeys.economy.all(guildId), "items"] as const,
	},

	leveling: {
		all: (guildId: string) => ["guilds", guildId, "leveling"] as const,
		config: (guildId: string) => [...queryKeys.leveling.all(guildId), "config"] as const,
		leaderboard: (guildId: string, limit: number) =>
			[...queryKeys.leveling.all(guildId), "leaderboard", limit] as const,
		rewards: (guildId: string, params: IListParams) =>
			[...queryKeys.leveling.all(guildId), "rewards", params] as const,
		rewardLists: (guildId: string) => [...queryKeys.leveling.all(guildId), "rewards"] as const,
	},

	welcome: {
		all: (guildId: string) => ["guilds", guildId, "welcome"] as const,
		config: (guildId: string) => [...queryKeys.welcome.all(guildId), "config"] as const,
		preview: (guildId: string) => [...queryKeys.welcome.all(guildId), "preview"] as const,
	},

	panels: {
		all: (guildId: string) => ["guilds", guildId, "panels"] as const,
		list: (guildId: string, params: IListParams) =>
			[...queryKeys.panels.all(guildId), "list", params] as const,
		detail: (guildId: string, panelId: string) =>
			[...queryKeys.panels.all(guildId), "detail", panelId] as const,
	},

	jail: {
		all: (guildId: string) => ["guilds", guildId, "jail"] as const,
		config: (guildId: string) => [...queryKeys.jail.all(guildId), "config"] as const,
		sentences: (guildId: string) => [...queryKeys.jail.all(guildId), "sentences"] as const,
	},

	autoroles: {
		all: (guildId: string) => ["guilds", guildId, "autoroles"] as const,
		list: (guildId: string, params: IListParams) =>
			[...queryKeys.autoroles.all(guildId), "list", params] as const,
		lists: (guildId: string) => [...queryKeys.autoroles.all(guildId), "list"] as const,
	},
} as const

/** Every key under a guild, for a full invalidation after a destructive change. */
export function guildQueryKey(guildId: string): readonly unknown[] {
	return ["guilds", guildId]
}
