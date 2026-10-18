import { createFileRoute, type ErrorComponentProps, Outlet, redirect } from "@tanstack/react-router"
import { AppShell } from "#/components/layout/app-shell.tsx"
import { RouteError, RoutePending } from "#/components/layout/route-states.tsx"
import { sessionStore } from "#/libs/auth/session.ts"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { loadSession } from "#/routes/_authenticated/_data/session.ts"

/**
 * The guard for everything behind sign-in.
 *
 * This file is half of a pair: `_authenticated.tsx` (this guard plus the
 * `<Outlet/>`) and `_authenticated/` (the children it renders). The file alone
 * would be a leaf with no children; the folder alone would render routes with
 * no guard at all. Both must exist.
 *
 * Two ways in:
 *  - no refresh token on disk -> straight to `/`, no request wasted;
 *  - a token that turns out to be rejected -> the load throws, and the error
 *    component sends the user back to `/`.
 *
 * On success the resolved `{user, guilds}` is handed to every child through
 * router context, so no route below needs to fetch the session again.
 */
export const Route = createFileRoute("/_authenticated")({
	beforeLoad: async ({ context }) => {
		if (!sessionStore.hasSession()) {
			throw redirect({ to: "/", replace: true })
		}

		try {
			const me = await loadSession(context.queryClient)
			return {
				session: {
					user: me.user,
					guilds: me.guilds,
					guild: null,
				},
			}
		} catch (error) {
			// A 401 here means the refresh token was rejected and the client has
			// already cleared the session; anything else is a server fault and
			// should be shown rather than swallowed.
			if (normalizeApiError(error).status === 401) {
				throw redirect({ to: "/", replace: true })
			}
			throw error
		}
	},
	component: AuthenticatedLayout,
	pendingComponent: () => <RoutePending label="Loading your servers" />,
	errorComponent: AuthenticatedError,
})

function AuthenticatedLayout() {
	const { session } = Route.useRouteContext()

	return (
		<AppShell>
			{/* The session is guaranteed by `beforeLoad`, but the context type allows
			    null for the router's benefit; this narrowing keeps children honest. */}
			{session === null ? null : <Outlet />}
		</AppShell>
	)
}

function AuthenticatedError({ error, reset }: ErrorComponentProps) {
	return <RouteError message={normalizeApiError(error).message} onRetry={reset} />
}
