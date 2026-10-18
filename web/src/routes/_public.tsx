import { createFileRoute, Outlet } from "@tanstack/react-router"

/**
 * Layout for routes that must be reachable without a session.
 *
 * It exists mainly so the public branch is explicit in the route tree — the
 * signed-out half of the app is a first-class branch rather than a fallthrough.
 * Anything added under `_public/` (password reset, invite acceptance) inherits
 * the same contract: no guard, no guild context.
 */
export const Route = createFileRoute("/_public")({
	component: PublicLayout,
})

function PublicLayout() {
	return (
		<div className="flex min-h-svh flex-col bg-background">
			<Outlet />
		</div>
	)
}
