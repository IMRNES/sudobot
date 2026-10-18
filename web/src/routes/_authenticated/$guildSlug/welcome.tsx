import { createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { RouteForbidden } from "#/components/layout/route-states.tsx"
import { useCan } from "#/hooks/use-can.ts"
import { WelcomeConfigForm } from "#/routes/_authenticated/$guildSlug/_components/welcome-config-form.tsx"

/**
 * Welcome and goodbye messaging. The template editor validates placeholders
 * against the API's allow-list before submitting, so a typo is caught locally
 * rather than coming back as a bare 400.
 */
export const Route = createFileRoute("/_authenticated/$guildSlug/welcome")({
	component: WelcomePage,
})

function WelcomePage() {
	const { guildSlug } = Route.useParams()
	const { session } = Route.useRouteContext()
	const guild = session?.guild ?? null
	const read = useCan("welcome:read", guild)
	const write = useCan("welcome:write", guild)

	if (!read.allowed) return <RouteForbidden permission="welcome:read" />

	return (
		<div className="flex flex-col gap-6">
			<PageHeader
				title="Welcome and goodbye"
				description="Greet new members and acknowledge the ones who leave, in whichever channel you choose."
			/>
			<WelcomeConfigForm
				guildId={guildSlug}
				canWrite={write.allowed}
				deniedReason={write.reason || read.reason}
			/>
		</div>
	)
}
