import { useQuery } from "@tanstack/react-query"
import { Trophy } from "lucide-react"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { QueryBoundary } from "#/components/features/query-boundary.tsx"
import { UserCell } from "#/components/features/user-cell.tsx"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { cn, formatNumber } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

const MEDALS = [
	"bg-warning/20 text-warning",
	"bg-muted text-foreground",
	"bg-warning/10 text-warning",
]

/**
 * Top balances in the guild.
 *
 * `limit` is a query parameter on this route (the API documents `?limit=10`),
 * not a page, so the control is a small segmented picker rather than the shared
 * `<Pagination>` — there is nothing to page through.
 */
export function EconomyLeaderboard({
	guildId,
	limit,
	onLimitChange,
}: {
	guildId: string
	limit: number
	onLimitChange: (limit: number) => void
}) {
	const query = useQuery(guildQueries.economyLeaderboard(guildId, limit))
	const entries = query.data?.data ?? []

	return (
		<Card>
			<CardHeader className="flex-row items-start justify-between gap-4">
				<div className="flex flex-col gap-1">
					<CardTitle className="text-base">Richest members</CardTitle>
					<CardDescription>Current balances, highest first.</CardDescription>
				</div>
				<LimitPicker limit={limit} onChange={onLimitChange} />
			</CardHeader>
			<CardContent>
				<QueryBoundary
					isLoading={query.isLoading}
					isError={query.isError}
					error={query.error}
					isEmpty={entries.length === 0}
					empty={
						<EmptyState
							icon={Trophy}
							title="No balances yet"
							description="Once members start earning, the top balances show up here."
							className="py-8"
						/>
					}
				>
					<ol className="flex flex-col divide-y divide-border">
						{entries.map((entry) => (
							<li key={entry.discord_id} className="flex items-center gap-4 py-3">
								<span
									className={cn(
										"grid size-7 shrink-0 place-items-center rounded-full text-xs font-semibold tabular-nums",
										MEDALS[entry.rank - 1] ?? "bg-muted/50 text-muted-foreground",
									)}
								>
									{entry.rank}
								</span>
								<UserCell
									discordId={entry.discord_id}
									name={entry.display_name}
									className="flex-1"
								/>
								<span className="shrink-0 text-sm font-medium tabular-nums">
									{formatNumber(entry.balance)}
								</span>
							</li>
						))}
					</ol>
				</QueryBoundary>
			</CardContent>
		</Card>
	)
}

function LimitPicker({ limit, onChange }: { limit: number; onChange: (limit: number) => void }) {
	const options = [5, 10, 25]
	return (
		<div className="flex shrink-0 items-center gap-1 rounded-lg border border-border p-0.5">
			{options.map((option) => (
				<button
					key={option}
					type="button"
					onClick={() => onChange(option)}
					aria-pressed={option === limit}
					className={cn(
						"rounded-md px-2 py-1 text-xs font-medium transition-colors",
						option === limit
							? "bg-primary text-primary-foreground"
							: "text-muted-foreground hover:text-foreground",
					)}
				>
					{option}
				</button>
			))}
		</div>
	)
}
