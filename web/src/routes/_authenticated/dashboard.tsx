import { createFileRoute } from "@tanstack/react-router"
import { DashboardHeader, GuildGrid } from "#/routes/_authenticated/_components/guild-grid.tsx"

/**
 * Post-sign-in landing page.
 *
 * It reads the session straight from router context — the `_authenticated`
 * guard already loaded it — so there is no second request and no loading state
 * on the most-visited page in the app.
 */
export const Route = createFileRoute("/_authenticated/dashboard")({
	component: DashboardPage,
})

function DashboardPage() {
	const { session } = Route.useRouteContext()
	const guilds = session?.guilds ?? []

	return (
		<div className="flex flex-col gap-6">
			<DashboardHeader guildCount={guilds.length} />
			<GuildGrid guilds={guilds} />
		</div>
	)
}
