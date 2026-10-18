import { createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { RouteForbidden } from "#/components/layout/route-states.tsx"
import { useCan } from "#/hooks/use-can.ts"
import {
	PermissionMatrix,
	RolesPanel,
} from "#/routes/_authenticated/$guildSlug/_components/roles-panel.tsx"

/**
 * Roles: the roles themselves plus the permission matrix that decides what each
 * one can do. The matrix is the point of the page, so it comes first.
 */
export const Route = createFileRoute("/_authenticated/$guildSlug/roles")({
	component: RolesPage,
})

function RolesPage() {
	const { guildSlug } = Route.useParams()
	const { session } = Route.useRouteContext()
	const guild = session?.guild ?? null
	const read = useCan("roles:read", guild)
	const write = useCan("roles:write", guild)

	if (!read.allowed) return <RouteForbidden permission="roles:read" />
	const deniedReason = write.reason || read.reason

	return (
		<div className="flex flex-col gap-6">
			<PageHeader
				title="Roles and permissions"
				description="Dashboard roles are separate from Discord roles. They decide who can see and change what here."
			/>
			<PermissionMatrix guildId={guildSlug} canWrite={write.allowed} deniedReason={deniedReason} />
			<RolesPanel guildId={guildSlug} canWrite={write.allowed} deniedReason={deniedReason} />
		</div>
	)
}
