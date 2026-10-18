import { QueryClientProvider } from "@tanstack/react-query"
import { createRootRouteWithContext, Outlet } from "@tanstack/react-router"
import { Toaster } from "#/components/ui/sonner.tsx"
import { TooltipProvider } from "#/components/ui/tooltip.tsx"
import { SessionProvider } from "#/libs/auth/session-context.tsx"
import type { TAppRouterContext } from "#/libs/query/router-context.ts"

/**
 * Application root.
 *
 * Holds the two providers that must outlive every route: the query client
 * (supplied through router context so loaders share it) and the tooltip
 * provider, which every disabled-with-explanation control depends on.
 */
export const Route = createRootRouteWithContext<TAppRouterContext>()({
	component: RootComponent,
})

function RootComponent() {
	// The query client arrives through router context; `useRouteContext` would
	// work too, but destructuring the context object directly keeps the provider
	// unambiguous about which client it is handing down.
	const { queryClient } = Route.useRouteContext()

	return (
		<QueryClientProvider client={queryClient}>
			<SessionProvider>
				<TooltipProvider delayDuration={200}>
					<Outlet />
					<Toaster />
				</TooltipProvider>
			</SessionProvider>
		</QueryClientProvider>
	)
}
