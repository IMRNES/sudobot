import { QueryClient } from "@tanstack/react-query"
import { ApiError } from "#/libs/errors/api-error.ts"

/**
 * One `QueryClient` per browser tab.
 *
 * Retry policy is deliberately narrow: retrying a `4xx` only wastes a round
 * trip and, for a `403`, would hammer an endpoint the user cannot use. Only
 * transport faults and 5xx responses are retried.
 */
export function createQueryClient(): QueryClient {
	return new QueryClient({
		defaultOptions: {
			queries: {
				staleTime: 30_000,
				gcTime: 5 * 60_000,
				refetchOnWindowFocus: false,
				retry: (failureCount, error) => {
					if (error instanceof ApiError) {
						if (error.status === 0) return failureCount < 2
						if (error.status >= 400 && error.status < 500) return false
						return failureCount < 2
					}
					return failureCount < 2
				},
			},
			mutations: {
				retry: false,
			},
		},
	})
}

let client: QueryClient | null = null

/**
 * Process-wide singleton. Route loaders and components reach for the same
 * instance, so a loader's `ensureQueryData` warms the cache the component reads.
 */
export function getQueryClient(): QueryClient {
	if (client === null) client = createQueryClient()
	return client
}

/** Test/SSR escape hatch; unused in the browser build. */
export function resetQueryClient(): void {
	client?.clear()
	client = null
}
