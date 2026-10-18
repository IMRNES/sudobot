import { createFileRoute } from "@tanstack/react-router"
import { useState } from "react"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { RouteForbidden } from "#/components/layout/route-states.tsx"
import { useCan } from "#/hooks/use-can.ts"
import { LevelingConfigForm } from "#/routes/_authenticated/$guildSlug/_components/leveling-config-form.tsx"
import { LevelingLeaderboard } from "#/routes/_authenticated/$guildSlug/_components/leveling-leaderboard.tsx"
import { RewardsPanel } from "#/routes/_authenticated/$guildSlug/_components/rewards-panel.tsx"

/**
 * Leveling: XP configuration, the ranking, and the level-to-role rewards.
 *
 * Read access to the page is `leveling:read`; the config form and reward
 * controls are gated on `leveling:write` and render disabled-with-reason, so a
 * moderator can read the current curve without being able to change it.
 */
export const Route = createFileRoute("/_authenticated/$guildSlug/leveling")({
	component: LevelingPage,
})

function LevelingPage() {
	const { guildSlug } = Route.useParams()
	const { session } = Route.useRouteContext()
	const guild = session?.guild ?? null
	const read = useCan("leveling:read", guild)
	const write = useCan("leveling:write", guild)
	const writeReason = write.reason || read.reason

	const [limit, setLimit] = useState(25)

	// Guard after the hooks: an early return above them would change hook order.
	if (!read.allowed) return <RouteForbidden permission="leveling:read" />

	return (
		<div className="flex flex-col gap-6">
			<PageHeader
				title="Leveling"
				description="XP rates, the level curve and the roles members earn as they climb."
			/>

			<LevelingConfigForm guildId={guildSlug} canWrite={write.allowed} deniedReason={writeReason} />

			<RewardsPanel guildId={guildSlug} canWrite={write.allowed} deniedReason={writeReason} />

			<LevelingLeaderboard guildId={guildSlug} limit={limit} onLimitChange={setLimit} />
		</div>
	)
}
