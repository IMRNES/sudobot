import { Link } from "@tanstack/react-router"
import { ArrowRight, Bot, Users } from "lucide-react"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { Badge } from "#/components/ui/badge.tsx"
import { Button } from "#/components/ui/button.tsx"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import type { IGuildSummary } from "#/libs/api/types.ts"
import { formatNumber } from "#/libs/utils.ts"

/**
 * The server picker that greets a signed-in user.
 *
 * Every card links into the guild layout. Guilds where the bot is absent are
 * shown but not linked: the API reports them (`bot_present: false`) and hiding
 * them would leave an admin wondering why their server is missing.
 */
export function GuildGrid({ guilds, userName }: { guilds: IGuildSummary[]; userName: string }) {
	const manageable = guilds.filter((guild) => guild.bot_present)

	if (manageable.length === 0) {
		return (
			<Card>
				<CardContent className="px-6 py-2">
					<EmptyState
						icon={Bot}
						title="No servers with sudobot yet"
						description={`${userName}, invite the bot to a server you manage and it will appear here. Servers where you lack the Manage Server permission are not listed.`}
					/>
				</CardContent>
			</Card>
		)
	}

	return (
		<div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
			{manageable.map((guild) => (
				<GuildCard key={guild.id} guild={guild} />
			))}
		</div>
	)
}

export function DashboardHeader({ guildCount }: { guildCount: number }) {
	return (
		<PageHeader
			title="Your servers"
			description={`${formatNumber(guildCount)} ${guildCount === 1 ? "server" : "servers"} where you can manage sudobot.`}
		/>
	)
}

function GuildCard({ guild }: { guild: IGuildSummary }) {
	return (
		<Card className="group gap-4 py-5 transition-colors hover:border-primary/40">
			<CardHeader className="px-5">
				<CardTitle className="flex items-center gap-2 text-base">
					<Bot className="size-4 shrink-0 text-muted-foreground" />
					<span className="truncate">{guild.name}</span>
				</CardTitle>
				<CardDescription className="flex items-center gap-2">
					<Users className="size-3.5" />
					{formatNumber(guild.member_count)} members
				</CardDescription>
			</CardHeader>
			<CardContent className="flex items-center justify-between gap-3 px-5">
				<Badge variant="secondary" className="font-normal">
					{guild.role_name}
				</Badge>
				<Button asChild size="sm" variant="ghost" className="group-hover:bg-accent">
					<Link to="/$guildSlug" params={{ guildSlug: guild.id }}>
						Open
						<ArrowRight />
					</Link>
				</Button>
			</CardContent>
		</Card>
	)
}
