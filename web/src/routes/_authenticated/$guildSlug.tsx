import {
	createFileRoute,
	type ErrorComponentProps,
	Link,
	notFound,
	Outlet,
	redirect,
} from "@tanstack/react-router"
import { GuildBreadcrumb } from "#/components/layout/guild-breadcrumb.tsx"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { RouteError, RoutePending } from "#/components/layout/route-states.tsx"
import { Button } from "#/components/ui/button.tsx"
import { normalizeApiError } from "#/libs/errors/index.ts"

/**
 * Guild layout.
 *
 * `$guildSlug` is the guild id as a string (a snowflake — see API.md). Resolving
 * it against the session's guild list serves two purposes:
 *
 *  1. it is the access check. A guild the user is not a member of is simply not
 *     in `session.guilds`, so the lookup fails and the route 404s. The API would
 *     return a 403 on the first data request anyway, but catching it here means
 *     the user never sees a page frame that cannot fill itself in.
 *  2. it publishes `role_slug` and `permissions` into router context, which is
 *     what `useCan()` reads. One payload, shared by the whole subtree.
 *
 * This file pairs with `$guildSlug/` — the folder holds this layout's children.
 * The file alone renders no children; the folder alone has no guard.
 */
export const Route = createFileRoute("/_authenticated/$guildSlug")({
	beforeLoad: ({ context, params }) => {
		const session = context.session
		if (session === null) {
			// Defensive: the parent guard already guarantees this, but a loader
			// running out of order should not produce a confusing type error.
			throw redirect({ to: "/", replace: true })
		}

		const guild = session.guilds.find((entry) => entry.id === params.guildSlug)
		if (guild === undefined) {
			// Not one of theirs — indistinguishable from "does not exist", which is
			// exactly what we want to tell them.
			throw notFound()
		}

		if (!guild.bot_present) {
			throw new Error(
				`sudobot is not in ${guild.name}. Invite the bot to that server before managing it.`,
			)
		}

		return {
			session: { ...session, guild },
		}
	},
	component: GuildLayout,
	pendingComponent: () => <RoutePending label="Opening server" />,
	errorComponent: GuildError,
	notFoundComponent: GuildNotFound,
})

function GuildLayout() {
	const { session } = Route.useRouteContext()
	const { guildSlug } = Route.useParams()
	const guild = session?.guild ?? null

	return (
		<div className="flex flex-col gap-6">
			<GuildBreadcrumb guild={guild} guildSlug={guildSlug} />
			<Outlet />
		</div>
	)
}

function GuildError({ error, reset }: ErrorComponentProps) {
	return (
		<RouteError
			title="This server could not be opened"
			message={normalizeApiError(error).message}
			onRetry={reset}
		/>
	)
}

/**
 * A guild id that is not in the member's list, or a path segment that matches no
 * child route. Both are deliberately the same answer: telling somebody "you are
 * not a member of guild 123" for an arbitrary id would leak the existence of
 * other servers.
 */
function GuildNotFound() {
	return (
		<div className="mx-auto flex max-w-lg flex-col items-start gap-4 py-12">
			<PageHeader
				title="Page not found"
				description="This server is not one of yours, or the page you asked for does not exist."
			/>
			<Button asChild variant="outline">
				<Link to="/dashboard">Back to your servers</Link>
			</Button>
		</div>
	)
}
