import { createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { RouteForbidden } from "#/components/layout/route-states.tsx"
import { useCan } from "#/hooks/use-can.ts"
import { PanelsPanel } from "#/routes/_authenticated/$guildSlug/_components/panels-panel.tsx"

/**
 * Role panels: self-assign messages members click to give themselves roles.
 *
 * Gated on `autorole:read`/`autorole:write` rather than a permission of its own,
 * because panels and automatic roles are the same feature from the API's side —
 * `panel_routes` lives under the same permission pair.
 */
export const Route = createFileRoute("/_authenticated/$guildSlug/panels")({
	component: PanelsPage,
})

function PanelsPage() {
	const { guildSlug } = Route.useParams()
	const { session } = Route.useRouteContext()
	const guild = session?.guild ?? null
	const read = useCan("autorole:read", guild)
	const write = useCan("autorole:write", guild)

	if (!read.allowed) return <RouteForbidden permission="autorole:read" />

	return (
		<div className="flex flex-col gap-6">
			<PageHeader
				title="Role panels"
				description="Messages members click to give themselves roles, without waiting for a moderator."
			/>
			<PanelsPanel
				guildId={guildSlug}
				canWrite={write.allowed}
				deniedReason={write.reason || read.reason}
			/>
		</div>
	)
}
