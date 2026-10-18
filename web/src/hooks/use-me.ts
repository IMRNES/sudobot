import { useQuery } from "@tanstack/react-query"
import { authApi } from "#/libs/api/auth.ts"
import type { IGuildSummary, IMeResponse, IUser } from "#/libs/api/types.ts"
import { sessionStore } from "#/libs/auth/session.ts"
import { queryKeys } from "#/libs/query/keys.ts"

/**
 * The signed-in identity and the guilds they can see.
 *
 * This is the single source of truth for both: `_authenticated.tsx` gates on
 * it, `$guildSlug.tsx` resolves the URL slug against `guilds`, and `useCan`
 * reads the per-guild permission list from it. One query, no duplication.
 */
export function useMe() {
	return useQuery({
		queryKey: queryKeys.session.me(),
		queryFn: ({ signal }) => authApi.me(signal),
		// The session is the app's backbone: keep it fresh-ish, but never refetch
		// it on every window focus.
		staleTime: 60_000,
		retry: false,
		enabled: sessionStore.hasSession(),
	})
}

/** Convenience accessors so components do not repeat `me?.user` chains. */
export function useCurrentUser(): IUser | undefined {
	return useMe().data?.user
}

export function useGuilds(): IGuildSummary[] {
	return useMe().data?.guilds ?? []
}

export type TMeQuery = ReturnType<typeof useMe>
export type TMeData = IMeResponse
