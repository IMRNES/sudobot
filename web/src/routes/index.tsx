import { createFileRoute, redirect } from "@tanstack/react-router"
import { Coins, ShieldCheck, Sparkles, TrendingUp } from "lucide-react"
import { SignInButton } from "#/components/auth/sign-in-button.tsx"
import { Card, CardContent } from "#/components/ui/card.tsx"
import { sessionStore } from "#/libs/auth/session.ts"

/**
 * Landing page. Signed-in visitors never see it: the loader bounces them to the
 * dashboard before the component renders, so the marketing copy is not flashed
 * at someone who already has a session.
 */
export const Route = createFileRoute("/")({
	beforeLoad: () => {
		if (sessionStore.hasSession()) {
			throw redirect({ to: "/dashboard" })
		}
	},
	component: LandingPage,
})

const FEATURES = [
	{
		icon: Coins,
		title: "Economy",
		body: "Balances, a shop with role rewards, and an auditable transaction ledger.",
	},
	{
		icon: TrendingUp,
		title: "Leveling",
		body: "XP curves tuned per server, with role rewards that survive a demotion.",
	},
	{
		icon: ShieldCheck,
		title: "Roles",
		body: "A permission matrix for the dashboard, scoped to each of your servers.",
	},
	{
		icon: Sparkles,
		title: "Onboarding",
		body: "Welcome messages, self-assign role panels and automatic role assignment.",
	},
] as const

function LandingPage() {
	return (
		<div className="mx-auto flex min-h-svh w-full max-w-5xl flex-col px-6 py-16">
			<header className="flex items-center justify-between">
				<span className="text-sm font-semibold tracking-tight">sudobot</span>
				<SignInButton size="sm" variant="outline" label="Open dashboard" />
			</header>

			<section className="flex flex-1 flex-col justify-center gap-6 py-20">
				<p className="text-xs font-medium tracking-[0.2em] text-muted-foreground uppercase">
					Discord bot dashboard
				</p>
				<h1 className="max-w-3xl text-4xl font-semibold tracking-tight text-balance sm:text-5xl">
					Run your community&rsquo;s economy, levels and roles from one place.
				</h1>
				<p className="max-w-2xl text-base text-muted-foreground text-pretty">
					sudobot gives every server a dashboard for the parts of Discord that are painful to
					administer by hand — with per-role permissions, so moderators can do their job without
					holding the keys to everything.
				</p>
				<div className="flex flex-wrap gap-3 pt-2">
					<SignInButton />
				</div>
			</section>

			<section className="grid gap-4 sm:grid-cols-2">
				{FEATURES.map((feature) => (
					<Card key={feature.title} className="gap-3 py-5">
						<CardContent className="flex gap-3 px-5">
							<span className="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/12 text-primary">
								<feature.icon className="size-4" />
							</span>
							<div className="flex flex-col gap-1">
								<p className="text-sm font-medium">{feature.title}</p>
								<p className="text-sm text-muted-foreground">{feature.body}</p>
							</div>
						</CardContent>
					</Card>
				))}
			</section>
		</div>
	)
}
