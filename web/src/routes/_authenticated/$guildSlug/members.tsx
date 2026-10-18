import { createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { RouteForbidden } from "#/components/layout/route-states.tsx"
import { useCan } from "#/hooks/use-can.ts"
import { validateTableSearch } from "#/routes/_authenticated/_data/table-search.ts"
import { MembersPanel } from "#/routes/_authenticated/$guildSlug/_components/members-panel.tsx"

/** Members: the paginated list plus dashboard-role assignment. */
export const Route = createFileRoute("/_authenticated/$guildSlug/members")({
	validateSearch: validateTableSearch,
	component: MembersPage,
})

function MembersPage() {
	const { guildSlug } = Route.useParams()
	const { session } = Route.useRouteContext()
	const guild = session?.guild ?? null
	const read = useCan("members:read", guild)
	const write = useCan("members:write", guild)

	if (!read.allowed) return <RouteForbidden permission="members:read" />

	return (
		<div className="flex flex-col gap-6">
			<PageHeader
				title="Members"
				description="Everyone who has signed in to this server's dashboard, and the dashboard role each of them holds."
			/>
			<MembersPanel
				guildId={guildSlug}
				routeId="/_authenticated/$guildSlug/members"
				canWrite={write.allowed}
				deniedReason={write.reason || read.reason}
			/>
		</div>
	)
}
