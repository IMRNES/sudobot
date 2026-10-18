import { useQuery } from "@tanstack/react-query"
import { Search, Wallet } from "lucide-react"
import { useState } from "react"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { QueryBoundary } from "#/components/features/query-boundary.tsx"
import { Button } from "#/components/ui/button.tsx"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { Input } from "#/components/ui/input.tsx"
import { Label } from "#/components/ui/label.tsx"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { formatNumber, formatRelativeTime } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * Account lookup by Discord id.
 *
 * The API has no "search accounts by name" route — the only account key is
 * `discordId` — so this is an explicit id lookup rather than a fuzzy search,
 * and it says so. The id is validated for shape before the request goes out, so
 * a typo produces an inline message instead of a round trip.
 */
export function AccountLookup({
	guildId,
	onSelect,
	selectedId,
}: {
	guildId: string
	onSelect: (discordId: string) => void
	selectedId: string | null
}) {
	const [draft, setDraft] = useState("")
	const [submitted, setSubmitted] = useState<string | null>(selectedId)
	const [shapeError, setShapeError] = useState<string | null>(null)

	const query = useQuery(guildQueries.economyAccount(guildId, submitted ?? "", submitted !== null))
	const account = query.data

	const submit = () => {
		const candidate = draft.trim()
		if (!/^\d{17,20}$/.test(candidate)) {
			setShapeError(
				"A Discord id is 17 to 20 digits. Right-click a member with Developer Mode on to copy it.",
			)
			return
		}
		setShapeError(null)
		setSubmitted(candidate)
		onSelect(candidate)
	}

	const notFound =
		query.isError && normalizeApiError(query.error).status === 404 && submitted !== null

	return (
		<Card>
			<CardHeader>
				<CardTitle className="text-base">Account lookup</CardTitle>
				<CardDescription>
					Look up a single member&rsquo;s balance by their Discord id.
				</CardDescription>
			</CardHeader>
			<CardContent className="flex flex-col gap-4">
				<form
					className="flex flex-col gap-2 sm:flex-row sm:items-end"
					onSubmit={(event) => {
						event.preventDefault()
						submit()
					}}
				>
					<div className="flex flex-1 flex-col gap-2">
						<Label htmlFor="economy-discord-id">Discord id</Label>
						<Input
							id="economy-discord-id"
							value={draft}
							onChange={(event) => setDraft(event.target.value)}
							placeholder="123456789012345678"
							inputMode="numeric"
							autoComplete="off"
							aria-invalid={shapeError !== null}
							aria-describedby={shapeError !== null ? "economy-discord-id-error" : undefined}
						/>
					</div>
					<Button type="submit">
						<Search />
						Look up
					</Button>
				</form>

				{shapeError !== null ? (
					<p id="economy-discord-id-error" role="alert" className="text-xs text-destructive">
						{shapeError}
					</p>
				) : null}

				{submitted === null ? (
					<EmptyState
						icon={Wallet}
						title="No account selected"
						description="Enter a Discord id to see that member's balance and recent activity."
						className="py-8"
					/>
				) : (
					<QueryBoundary
						isLoading={query.isLoading}
						isError={query.isError && !notFound}
						error={query.error}
						isEmpty={notFound}
						empty={
							<EmptyState
								icon={Wallet}
								title="No account for that id"
								description="This member has not earned or spent anything yet, so no ledger row exists."
								className="py-8"
							/>
						}
					>
						{account ? (
							<div className="grid gap-4 rounded-lg border border-border bg-muted/25 p-4 sm:grid-cols-3">
								<Figure label="Balance" value={formatNumber(account.balance)} emphasis />
								<Figure label="Total earned" value={formatNumber(account.total_earned)} />
								<Figure label="Total spent" value={formatNumber(account.total_spent)} />
								<p className="text-xs text-muted-foreground sm:col-span-3">
									Last daily claim: {formatRelativeTime(account.last_daily_at)}
								</p>
							</div>
						) : null}
					</QueryBoundary>
				)}
			</CardContent>
		</Card>
	)
}

function Figure({
	label,
	value,
	emphasis = false,
}: {
	label: string
	value: string
	emphasis?: boolean
}) {
	return (
		<div className="flex flex-col gap-0.5">
			<span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
				{label}
			</span>
			<span className={emphasis ? "text-xl font-semibold tabular-nums" : "text-lg tabular-nums"}>
				{value}
			</span>
		</div>
	)
}
