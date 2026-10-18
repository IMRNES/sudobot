import { Link } from "@tanstack/react-router"
import { Check, ChevronsUpDown, Plus } from "lucide-react"
import { Avatar, AvatarFallback, AvatarImage } from "#/components/ui/avatar.tsx"
import { Badge } from "#/components/ui/badge.tsx"
import { Button } from "#/components/ui/button.tsx"
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "#/components/ui/dropdown-menu.tsx"
import type { IGuildSummary } from "#/libs/api/types.ts"
import { avatarUrl, cn, initialsOf } from "#/libs/utils.ts"

/**
 * Guild switcher.
 *
 * Sits at the top of the sidebar and shows the bot's presence for every guild:
 * a guild the bot has not joined can still be listed (the API returns
 * `bot_present: false`) but is not selectable, because every guild-scoped route
 * needs the bot's own role to exist.
 */
export function GuildSwitcher({
	guilds,
	activeGuild,
	onNavigate,
}: {
	guilds: IGuildSummary[]
	activeGuild: IGuildSummary | null
	onNavigate?: () => void
}) {
	const available = guilds.filter((guild) => guild.bot_present)

	if (available.length === 0) {
		return (
			<div className="rounded-lg border border-sidebar-border bg-sidebar-accent/40 p-3">
				<p className="text-xs font-medium text-sidebar-foreground">No servers yet</p>
				<p className="mt-1 text-xs text-muted-foreground">
					Invite the bot to a server you manage, then reload this page.
				</p>
			</div>
		)
	}

	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button
					variant="ghost"
					className="h-auto w-full justify-between gap-2 px-2 py-2 hover:bg-sidebar-accent"
					aria-label="Switch server"
				>
					<span className="flex min-w-0 items-center gap-2">
						<GuildIcon guild={activeGuild} />
						<span className="min-w-0 text-left">
							<span className="block truncate text-sm font-medium">
								{activeGuild?.name ?? "Select a server"}
							</span>
							<span className="block truncate text-xs text-muted-foreground">
								{activeGuild
									? `${activeGuild.role_name} · ${activeGuild.member_count} members`
									: `${available.length} available`}
							</span>
						</span>
					</span>
					<ChevronsUpDown className="size-4 shrink-0 text-muted-foreground" />
				</Button>
			</DropdownMenuTrigger>

			<DropdownMenuContent align="start" className="w-64">
				<DropdownMenuLabel>Your servers</DropdownMenuLabel>
				<DropdownMenuSeparator />
				{available.map((guild) => (
					<DropdownMenuItem key={guild.id} asChild>
						<Link
							to="/$guildSlug"
							params={{ guildSlug: guild.id }}
							onClick={onNavigate}
							className="flex items-center gap-2"
						>
							<GuildIcon guild={guild} />
							<span className="min-w-0 flex-1">
								<span className="block truncate text-sm">{guild.name}</span>
								<span className="block truncate text-xs text-muted-foreground">
									{guild.role_name}
								</span>
							</span>
							{guild.id === activeGuild?.id ? (
								<Check className="size-4 text-primary" />
							) : (
								<Badge variant="outline" className="text-[10px]">
									{guild.role_slug}
								</Badge>
							)}
						</Link>
					</DropdownMenuItem>
				))}
				<DropdownMenuSeparator />
				<DropdownMenuItem asChild>
					<Link to="/dashboard" onClick={onNavigate} className="flex items-center gap-2">
						<Plus className="size-4" />
						<span className="text-sm">Add another server</span>
					</Link>
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	)
}

function GuildIcon({ guild, className }: { guild: IGuildSummary | null; className?: string }) {
	const icon = avatarUrl(guild?.icon_url, 64)
	return (
		<Avatar className={cn("size-7 rounded-md", className)}>
			{icon !== null ? <AvatarImage src={icon} alt="" /> : null}
			<AvatarFallback className="rounded-md bg-primary/15 text-[11px] font-semibold text-primary">
				{initialsOf(guild?.name ?? "?")}
			</AvatarFallback>
		</Avatar>
	)
}
