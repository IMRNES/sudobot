import { useQuery } from "@tanstack/react-query"
import { TrendingUp } from "lucide-react"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { QueryBoundary } from "#/components/features/query-boundary.tsx"
import { UserCell } from "#/components/features/user-cell.tsx"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { cn, formatNumber, formatPercent } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/** Top levelled members, with each member's progress toward the next level. */
export function LevelingLeaderboard({
	guildId,
	limit,
	onLimitChange,
}: {
	guildId: string
	limit: number
	onLimitChange: (limit: number) => void
}) {
	const query = useQuery(guildQueries.levelingLeaderboard(guildId, limit))
	const entries = query.data?.data ?? []

	return (
		<Card>
			<CardHeader className="flex-row items-start justify-between gap-4">
				<div className="flex flex-col gap-1">
					<CardTitle className="text-base">Leaderboard</CardTitle>
					<CardDescription>Ranked by total XP earned in this server.</CardDescription>
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
							icon={TrendingUp}
							title="No XP earned yet"
							description="Once members start chatting, the ranking builds itself."
							className="py-8"
						/>
					}
				>
					<ol className="flex flex-col divide-y divide-border">
						{entries.map((entry) => (
							<li key={entry.discord_id} className="flex items-center gap-4 py-3">
								<span
									className={cn(
										"w-6 shrink-0 text-center text-sm font-semibold tabular-nums",
										entry.rank <= 3 ? "text-primary" : "text-muted-foreground",
									)}
								>
									{entry.rank}
								</span>
								<UserCell
									discordId={entry.discord_id}
									// A member with no recorded name keeps the id fallback; showing
									// a name the server never told us would be a fabrication.
									name={entry.display_name}
									secondary={`Level ${entry.level}`}
									className="flex-1"
								/>
								<div className="flex w-28 shrink-0 flex-col items-end gap-1">
									<span className="text-sm font-medium tabular-nums">
										{formatNumber(entry.total_xp)} XP
									</span>
									<span className="flex w-full items-center gap-1.5">
										<span className="h-1 flex-1 overflow-hidden rounded-full bg-muted">
											<span
												className="block h-full rounded-full bg-primary"
												style={{ width: `${Math.round(entry.progress * 100)}%` }}
											/>
										</span>
										<span className="text-[10px] tabular-nums text-muted-foreground">
											{formatPercent(entry.progress)}
										</span>
									</span>
								</div>
							</li>
						))}
					</ol>
				</QueryBoundary>
			</CardContent>
		</Card>
	)
}

function LimitPicker({ limit, onChange }: { limit: number; onChange: (limit: number) => void }) {
	const options = [10, 25, 50]
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
