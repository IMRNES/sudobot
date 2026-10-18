import type { QueryClient } from "@tanstack/react-query"
import type { IGuildSummary, IUser } from "#/libs/api/types.ts"

/**
 * What every route receives through router context.
 *
 * `queryClient` and `session` are static for the life of the app; `user` and
 * `guilds` are snapshots taken by the `_authenticated` guard's loader, which is
 * what lets a child route read them synchronously without another hook call.
 */
export interface IAppSession {
	user: IUser
	guilds: IGuildSummary[]
	/** The guild whose id matches the current `$guildSlug`, when in one. */
	guild: IGuildSummary | null
}

export interface TAppRouterContext {
	queryClient: QueryClient
	/** `null` until the guard has resolved; always set inside `_authenticated`. */
	session: IAppSession | null
}
