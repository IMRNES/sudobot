import { createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { RouteForbidden } from "#/components/layout/route-states.tsx"
import { useCan } from "#/hooks/use-can.ts"
import { JailPanel } from "#/routes/_authenticated/$guildSlug/_components/jail-panel.tsx"

/**
 * Jail: the role that seals a member out of every channel except one, and the ways
 * back out.
 *
 * Gated on `members:read`/`members:write` because a jail is a moderation action and
 * those are the permissions that already cover acting on a member.
 */
export const Route = createFileRoute("/_authenticated/$guildSlug/jail")({
	component: JailPage,
})

function JailPage() {
	const { guildSlug } = Route.useParams()
	const { session } = Route.useRouteContext()
	const guild = session?.guild ?? null
	const read = useCan("members:read", guild)
	const write = useCan("members:write", guild)

	if (!read.allowed) return <RouteForbidden permission="members:read" />

	return (
		<div className="flex flex-col gap-6">
			<PageHeader
				title="Jail"
				description="A role that denies every channel except one, so a member can be removed from the server without being kicked from it."
			/>
			<JailPanel guildId={guildSlug} canWrite={write.allowed} />
		</div>
	)
}
