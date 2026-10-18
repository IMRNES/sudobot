import { createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { RouteForbidden } from "#/components/layout/route-states.tsx"
import { useCan } from "#/hooks/use-can.ts"
import { validateTableSearch } from "#/routes/_authenticated/_data/table-search.ts"
import { AutorolesPanel } from "#/routes/_authenticated/$guildSlug/_components/autoroles-panel.tsx"

/**
 * Autoroles: roles handed out automatically, either when a member joins, after
 * a delay, or once they are confirmed human.
 */
export const Route = createFileRoute("/_authenticated/$guildSlug/autoroles")({
	validateSearch: validateTableSearch,
	component: AutorolesPage,
})

function AutorolesPage() {
	const { guildSlug } = Route.useParams()
	const { session } = Route.useRouteContext()
	const guild = session?.guild ?? null
	const read = useCan("autorole:read", guild)
	const write = useCan("autorole:write", guild)

	if (!read.allowed) return <RouteForbidden permission="autorole:read" />

	return (
		<div className="flex flex-col gap-6">
			<PageHeader
				title="Autoroles"
				description="Roles sudobot grants on its own — on join, after a delay, or once a member passes the human check."
			/>
			<AutorolesPanel
				guildId={guildSlug}
				routeId="/_authenticated/$guildSlug/autoroles"
				canWrite={write.allowed}
				deniedReason={write.reason || read.reason}
			/>
		</div>
	)
}
