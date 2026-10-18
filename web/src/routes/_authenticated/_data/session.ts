import type { QueryClient } from "@tanstack/react-query"
import { authApi } from "#/libs/api/auth.ts"
import type { IMeResponse } from "#/libs/api/types.ts"
import { queryKeys } from "#/libs/query/keys.ts"

/**
 * Loader for the session root.
 *
 * `ensureQueryData` rather than `fetchQuery`: if the OAuth callback already
 * seeded `/me`, this resolves from cache with no request at all. The query is
 * the same one `useMe()` reads, so the guard and every component below it share
 * a single cache entry.
 */
export function loadSession(queryClient: QueryClient): Promise<IMeResponse> {
	return queryClient.ensureQueryData({
		queryKey: queryKeys.session.me(),
		queryFn: ({ signal }) => authApi.me(signal),
		staleTime: 60_000,
	})
}

/** Drop the cached session after a sign-out. */
export function clearSessionCache(queryClient: QueryClient): void {
	queryClient.removeQueries({ queryKey: queryKeys.session.all })
}
