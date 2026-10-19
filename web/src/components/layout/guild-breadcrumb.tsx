import { Link } from "@tanstack/react-router"
import {
	Breadcrumb,
	BreadcrumbItem,
	BreadcrumbLink,
	BreadcrumbList,
	BreadcrumbPage,
	BreadcrumbSeparator,
} from "#/components/ui/breadcrumb.tsx"
import type { IGuildSummary } from "#/libs/api/types.ts"

const LABELS: Record<string, string> = {
	economy: "Economy",
	leveling: "Leveling",
	members: "Members",
	roles: "Roles",
	welcome: "Welcome",
	panels: "Role panels",
	autoroles: "Autoroles",
	settings: "Settings",
}

/**
 * `Servers / Guild name / Section`, rendered above each guild page's title.
 * The section label comes from a static map rather than from the URL segment,
 * so it can read as prose.
 */
export function GuildBreadcrumb({
	guild,
	section,
	guildSlug,
}: {
	guild: IGuildSummary | null
	/** Last segment of the current route; omit on the guild overview. */
	section?: string
	guildSlug: string
}) {
	return (
		<Breadcrumb>
			<BreadcrumbList>
				<BreadcrumbItem>
					<BreadcrumbLink asChild>
						<Link to="/dashboard">Servers</Link>
					</BreadcrumbLink>
				</BreadcrumbItem>
				<BreadcrumbSeparator />
				<BreadcrumbItem>
					{section === undefined ? (
						<BreadcrumbPage>{guild?.name ?? "Server"}</BreadcrumbPage>
					) : (
						<BreadcrumbLink asChild>
							<Link to="/$guildSlug" params={{ guildSlug }}>
								{guild?.name ?? "Server"}
							</Link>
						</BreadcrumbLink>
					)}
				</BreadcrumbItem>
				{section !== undefined ? (
					<>
						<BreadcrumbSeparator />
						<BreadcrumbItem>
							<BreadcrumbPage>{LABELS[section] ?? section}</BreadcrumbPage>
						</BreadcrumbItem>
					</>
				) : null}
			</BreadcrumbList>
		</Breadcrumb>
	)
}
