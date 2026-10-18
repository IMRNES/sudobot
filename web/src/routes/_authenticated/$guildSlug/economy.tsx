import { useQuery } from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import { History } from "lucide-react"
import { useState } from "react"
import { DataTable, type IDataTableColumn } from "#/components/features/data-table.tsx"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { Pagination } from "#/components/features/pagination.tsx"
import { QueryBoundary } from "#/components/features/query-boundary.tsx"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { RouteForbidden } from "#/components/layout/route-states.tsx"
import { Card, CardContent, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { useCan } from "#/hooks/use-can.ts"
import { useTableSearch } from "#/hooks/use-table-search.ts"
import type { ITransactionResponse } from "#/libs/api/types.ts"
import { formatNumber, formatRelativeTime, formatSigned, shortId } from "#/libs/utils.ts"
import { validateBasicTableSearch } from "#/routes/_authenticated/_data/table-search.ts"
import { AccountLookup } from "#/routes/_authenticated/$guildSlug/_components/account-lookup.tsx"
import { EconomyLeaderboard } from "#/routes/_authenticated/$guildSlug/_components/economy-leaderboard.tsx"
import { ShopItemsPanel } from "#/routes/_authenticated/$guildSlug/_components/shop-items-panel.tsx"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * Economy: leaderboard, single-account lookup with its ledger, and shop CRUD.
 *
 * The page is readable to anyone with `economy:read`; every write control is
 * disabled (with a reason) rather than hidden when `economy:write` is missing.
 * The sidebar hides the entry entirely for members with neither, so a
 * completely unrelated role never lands here.
 */
export const Route = createFileRoute("/_authenticated/$guildSlug/economy")({
	validateSearch: validateBasicTableSearch,
	component: EconomyPage,
})

function EconomyPage() {
	const { guildSlug } = Route.useParams()
	const { session } = Route.useRouteContext()
	const guild = session?.guild ?? null
	const read = useCan("economy:read", guild)
	const write = useCan("economy:write", guild)

	const [leaderboardLimit, setLeaderboardLimit] = useState(10)
	const [selectedAccount, setSelectedAccount] = useState<string | null>(null)

	// Every request on this page is guild-scoped and gated on this permission, so
	// refuse the page rather than rendering a shell that would fill up with 403s.
	// The guard sits after the hooks: an early return before them would change the
	// hook order between renders.
	if (!read.allowed) return <RouteForbidden permission="economy:read" />

	return (
		<div className="flex flex-col gap-6">
			<PageHeader
				title="Economy"
				description="Balances, the transaction ledger and the shop members spend their currency in."
			/>

			<div className="grid gap-6 lg:grid-cols-2">
				<EconomyLeaderboard
					guildId={guildSlug}
					limit={leaderboardLimit}
					onLimitChange={setLeaderboardLimit}
				/>
				<AccountLookup
					guildId={guildSlug}
					selectedId={selectedAccount}
					onSelect={setSelectedAccount}
				/>
			</div>

			{selectedAccount !== null ? (
				<TransactionLedger guildId={guildSlug} discordId={selectedAccount} />
			) : null}

			<ShopItemsPanel
				guildId={guildSlug}
				routeId="/_authenticated/$guildSlug/economy"
				canWrite={write.allowed}
				deniedReason={write.reason || read.reason}
			/>
		</div>
	)
}

/**
 * The ledger for the account currently selected above.
 *
 * Only mounted once an account is chosen, which is why it owns its own
 * pagination state: it is a drill-down, not part of the page's URL contract.
 */
function TransactionLedger({ guildId, discordId }: { guildId: string; discordId: string }) {
	const table = useTableSearch("/_authenticated/$guildSlug/economy", { defaultPerPage: 10 })
	const query = useQuery(guildQueries.transactions(guildId, discordId, table.params))
	const transactions = query.data?.data ?? []
	const meta = query.data?.meta

	const columns: IDataTableColumn<ITransactionResponse>[] = [
		{
			id: "reason",
			header: "Reason",
			cell: (entry) => <span className="text-sm">{entry.reason}</span>,
		},
		{
			id: "amount",
			header: "Amount",
			align: "right",
			cell: (entry) => (
				<span
					className={
						entry.amount < 0 ? "tabular-nums text-destructive" : "tabular-nums text-success"
					}
				>
					{formatSigned(entry.amount)}
				</span>
			),
		},
		{
			id: "balance",
			header: "Balance after",
			align: "right",
			cell: (entry) => <span className="tabular-nums">{formatNumber(entry.balance_after)}</span>,
		},
		{
			id: "actor",
			header: "Actor",
			cell: (entry) =>
				entry.actor_discord_id === null ? (
					<span className="text-muted-foreground">System</span>
				) : (
					<span className="font-mono text-xs">{shortId(entry.actor_discord_id)}</span>
				),
		},
		{
			id: "created",
			header: "When",
			align: "right",
			cell: (entry) => (
				<span className="text-xs text-muted-foreground">
					{formatRelativeTime(entry.created_at)}
				</span>
			),
		},
	]

	return (
		<Card>
			<CardHeader>
				<CardTitle className="text-base">
					Ledger for <span className="font-mono text-sm">{shortId(discordId)}</span>
				</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-col gap-2 px-0">
				<QueryBoundary
					isLoading={query.isLoading}
					isError={query.isError}
					error={query.error}
					isEmpty={transactions.length === 0}
					empty={
						<EmptyState
							icon={History}
							title="No transactions yet"
							description="Adjustments, purchases and daily claims will appear here."
							className="py-10"
						/>
					}
				>
					<DataTable
						columns={columns}
						rows={transactions}
						getRowId={(entry) => entry.id}
						className="rounded-none border-x-0"
					/>
					{meta ? (
						<Pagination
							meta={meta}
							onPageChange={table.setPage}
							onPerPageChange={table.setPerPage}
							className="px-6"
						/>
					) : null}
				</QueryBoundary>
			</CardContent>
		</Card>
	)
}
