import { Link, useParams } from "@tanstack/react-router"
import {
	Coins,
	Gauge,
	LayoutDashboard,
	Lock,
	MousePointerClick,
	ScrollText,
	Settings,
	ShieldCheck,
	Sparkles,
	TrendingUp,
	UserPlus,
	Users,
} from "lucide-react"
import { GuildSwitcher } from "#/components/layout/guild-switcher.tsx"
import { UserMenu } from "#/components/layout/user-menu.tsx"
import { Separator } from "#/components/ui/separator.tsx"
import { useCanFn } from "#/hooks/use-can.ts"
import { useGuilds } from "#/hooks/use-me.ts"
import type { IGuildSummary } from "#/libs/api/types.ts"
import { cn } from "#/libs/utils.ts"

interface INavItem {
	label: string
	/** Path segment appended to `/$guildSlug`; empty for the overview. */
	segment: string
	icon: React.ComponentType<{ className?: string }>
	/** Permission required to *see* the entry. Absent means always visible. */
	permission?: string
}

const GUILD_NAV: INavItem[] = [
	{ label: "Overview", segment: "", icon: Gauge },
	{ label: "Economy", segment: "economy", icon: Coins, permission: "economy:read" },
	{ label: "Leveling", segment: "leveling", icon: TrendingUp, permission: "leveling:read" },
	{ label: "Members", segment: "members", icon: Users, permission: "members:read" },
	{ label: "Roles", segment: "roles", icon: ShieldCheck, permission: "roles:read" },
	{ label: "Welcome", segment: "welcome", icon: Sparkles, permission: "welcome:read" },
	{ label: "Role panels", segment: "panels", icon: MousePointerClick, permission: "autorole:read" },
	{ label: "Autoroles", segment: "autoroles", icon: UserPlus, permission: "autorole:read" },
	{ label: "Jail", segment: "jail", icon: Lock, permission: "members:read" },
	{ label: "Settings", segment: "settings", icon: Settings, permission: "dashboard:manage" },
]

/**
 * Primary navigation.
 *
 * Two modes: with no guild selected it lists the account-level destinations,
 * and inside a guild it lists that guild's feature pages. Entries the member
 * cannot read are hidden rather than disabled — a nav full of dead links is
 * noise — but individual *controls* on the pages themselves are disabled with a
 * tooltip, so a read-only member still learns what exists.
 */
export function Sidebar({
	guildSlug,
	onNavigate,
}: {
	guildSlug?: string
	onNavigate?: () => void
}) {
	const guilds = useGuilds()
	const params = useParams({ strict: false }) as { guildSlug?: string }
	const activeGuildId = guildSlug ?? params.guildSlug
	const guild = activeGuildId ? (guilds.find((entry) => entry.id === activeGuildId) ?? null) : null

	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="flex h-14 shrink-0 items-center gap-2 px-4">
				<Link
					to="/dashboard"
					onClick={onNavigate}
					className="flex items-center gap-2 text-sm font-semibold tracking-tight"
				>
					<span className="grid size-6 place-items-center rounded-md bg-primary text-primary-foreground">
						<ScrollText className="size-3.5" />
					</span>
					sudobot
				</Link>
			</div>

			<Separator className="bg-sidebar-border" />

			<div className="p-3">
				<GuildSwitcher guilds={guilds} activeGuild={guild} onNavigate={onNavigate} />
			</div>

			<nav className="min-h-0 flex-1 overflow-y-auto px-3 pb-3" aria-label="Guild sections">
				{guild ? (
					<GuildNav guild={guild} onNavigate={onNavigate} />
				) : (
					<AccountNav onNavigate={onNavigate} />
				)}
			</nav>

			<Separator className="bg-sidebar-border" />
			<div className="p-3">
				<UserMenu />
			</div>
		</div>
	)
}

function AccountNav({ onNavigate }: { onNavigate?: () => void }) {
	return (
		<ul className="flex flex-col gap-0.5">
			<NavLink to="/dashboard" icon={LayoutDashboard} onClick={onNavigate}>
				Your servers
			</NavLink>
		</ul>
	)
}

function GuildNav({ guild, onNavigate }: { guild: IGuildSummary; onNavigate?: () => void }) {
	const can = useCanFn(guild)
	const visible = GUILD_NAV.filter((item) => item.permission === undefined || can(item.permission))

	return (
		<ul className="flex flex-col gap-0.5">
			{visible.map((item) => (
				<NavLink
					key={item.label}
					to={item.segment === "" ? "/$guildSlug" : `/$guildSlug/${item.segment}`}
					params={{ guildSlug: guild.id }}
					icon={item.icon}
					// The overview is the parent path of every sibling, so only it
					// needs an exact match; the others are distinct leaves and a
					// prefix match cannot collide with one another.
					exact={item.segment === ""}
					onClick={onNavigate}
				>
					{item.label}
				</NavLink>
			))}
		</ul>
	)
}

function NavLink({
	to,
	params,
	icon: Icon,
	exact = false,
	children,
	onClick,
}: {
	to: string
	params?: Record<string, string>
	icon: React.ComponentType<{ className?: string }>
	/** Match the whole path rather than treating this link as a prefix. */
	exact?: boolean
	children: React.ReactNode
	onClick?: () => void
}) {
	return (
		<li>
			<Link
				// The generated route tree types these paths; the cast keeps this
				// helper usable from both nav modes without duplicating it.
				to={to as never}
				params={params as never}
				onClick={onClick}
				activeOptions={{ exact }}
				className={cn(
					"flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm font-medium text-sidebar-foreground/75 transition-colors",
					"hover:bg-sidebar-accent hover:text-sidebar-foreground",
					"data-[status=active]:bg-sidebar-accent data-[status=active]:text-sidebar-foreground",
				)}
			>
				<Icon className="size-4 shrink-0" />
				{children}
			</Link>
		</li>
	)
}
