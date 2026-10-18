import { createRouter } from "@tanstack/react-router"
import { getQueryClient } from "#/libs/query/query-client.ts"
import type { TAppRouterContext } from "#/libs/query/router-context.ts"
import { routeTree } from "#/routeTree.gen.ts"

/**
 * The router instance, created once per browser tab.
 *
 * The query client is injected as router context rather than imported directly
 * by loaders, which keeps loaders testable and guarantees they warm exactly the
 * cache the components read from.
 */
export const router = createRouter({
	routeTree,
	context: {
		queryClient: getQueryClient(),
		session: null,
	} satisfies TAppRouterContext,
	// Route loaders run once when their params change; the query cache handles
	// staleness from there, so a re-render does not re-fetch.
	defaultPreload: "intent",
	defaultPreloadStaleTime: 30_000,
	scrollRestoration: true,
	defaultPendingMs: 150,
	defaultPendingMinMs: 300,
})

declare module "@tanstack/react-router" {
	interface Register {
		router: typeof router
	}
}
