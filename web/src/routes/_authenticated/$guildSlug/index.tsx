import { createFileRoute } from "@tanstack/react-router"
import { Coins, ShieldCheck, TrendingUp, UserPlus, Users } from "lucide-react"
import { StatCard } from "#/components/features/stat-card.tsx"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { Badge } from "#/components/ui/badge.tsx"
import { useCanFn } from "#/hooks/use-can.ts"
import { formatNumber } from "#/libs/utils.ts"

/**
 * Guild overview.
 *
 * Every figure here is either already in the session payload (member count,
 * role) or a static fact about the guild, so this page issues no requests at
 * all. The feature pages carry the live data.
 */
export const Route = createFileRoute("/_authenticated/$guildSlug/")({
	component: GuildOverviewPage,
})

function GuildOverviewPage() {
	const { session } = Route.useRouteContext()
	const guild = session?.guild ?? null
	const can = useCanFn(guild)

	if (guild === null) return null

	const granted = guild.permissions.length

	return (
		<div className="flex flex-col gap-6">
			<PageHeader
				title={guild.name}
				description="Everything sudobot manages in this server, and what your role is allowed to change."
				badge={
					<Badge variant="secondary" className="font-normal">
						{guild.role_name}
					</Badge>
				}
			/>

			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
				<StatCard label="Members" value={formatNumber(guild.member_count)} icon={Users} />
				<StatCard
					label="Your role"
					value={guild.role_slug}
					hint={guild.role_name}
					icon={ShieldCheck}
				/>
				<StatCard
					label="Permissions"
					value={formatNumber(granted)}
					hint={granted === 0 ? "Read-only in this server" : "Granted to your role"}
					icon={Coins}
				/>
				<StatCard
					label="Server id"
					value={<span className="font-mono text-base">{guild.id}</span>}
					hint="Snowflake, stored as a string"
				/>
			</div>

			<section className="grid gap-4 sm:grid-cols-3">
				<Capability
					icon={Coins}
					title="Economy"
					enabled={can("economy:read")}
					canWrite={can("economy:write")}
				/>
				<Capability
					icon={TrendingUp}
					title="Leveling"
					enabled={can("leveling:read")}
					canWrite={can("leveling:write")}
				/>
				<Capability
					icon={UserPlus}
					title="Autoroles"
					enabled={can("autorole:read")}
					canWrite={can("autorole:write")}
				/>
			</section>
		</div>
	)
}

/**
 * One feature's availability, stated plainly. A member without a permission
 * should be able to see that the feature exists and that someone else can turn
 * it on — that is the difference between "not for you" and "not here".
 */
function Capability({
	icon: Icon,
	title,
	enabled,
	canWrite,
}: {
	icon: React.ComponentType<{ className?: string }>
	title: string
	enabled: boolean
	canWrite: boolean
}) {
	return (
		<div className="flex items-center gap-3 rounded-xl border border-border bg-card p-4">
			<span className="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/12 text-primary">
				<Icon className="size-4" />
			</span>
			<div className="min-w-0 flex-1">
				<p className="text-sm font-medium">{title}</p>
				<p className="text-xs text-muted-foreground">
					{!enabled ? "No access" : canWrite ? "Read and write" : "Read only"}
				</p>
			</div>
			<Badge variant={canWrite ? "default" : "outline"} className="shrink-0 font-normal">
				{!enabled ? "Hidden" : canWrite ? "Manage" : "View"}
			</Badge>
		</div>
	)
}
