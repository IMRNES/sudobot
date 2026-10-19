import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Minus, Plus, Search, Wallet } from "lucide-react"
import { useState } from "react"
import { toast } from "sonner"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { FormField } from "#/components/features/form-field.tsx"
import { PermissionButton } from "#/components/features/permission-button.tsx"
import { QueryBoundary } from "#/components/features/query-boundary.tsx"
import { Button } from "#/components/ui/button.tsx"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "#/components/ui/dialog.tsx"
import { Input } from "#/components/ui/input.tsx"
import { Label } from "#/components/ui/label.tsx"
import { economyApi } from "#/libs/api/resources.ts"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { formatNumber, formatRelativeTime } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * Account lookup by Discord id, with the one write an admin needs here.
 *
 * The API has no "search accounts by name" route — the only account key is
 * `discordId` — so this is an explicit id lookup rather than a fuzzy search, and
 * it says so. The id is validated for shape before the request goes out, so a typo
 * produces an inline message instead of a round trip.
 */
export function AccountLookup({
	guildId,
	onSelect,
	selectedId,
	canWrite,
	deniedReason,
}: {
	guildId: string
	onSelect: (discordId: string) => void
	selectedId: string | null
	canWrite: boolean
	deniedReason: string
}) {
	const [draft, setDraft] = useState("")
	const [submitted, setSubmitted] = useState<string | null>(selectedId)
	const [shapeError, setShapeError] = useState<string | null>(null)
	const [adjusting, setAdjusting] = useState(false)

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
								description="This member has not earned or spent anything yet. Adjusting their balance creates the account."
								className="py-8"
							/>
						}
					>
						{account ? (
							<div className="flex flex-col gap-4 rounded-lg border border-border bg-muted/25 p-4">
								<div className="grid gap-4 sm:grid-cols-3">
									<Figure label="Balance" value={formatNumber(account.balance)} emphasis />
									<Figure label="Total earned" value={formatNumber(account.total_earned)} />
									<Figure label="Total spent" value={formatNumber(account.total_spent)} />
								</div>
								<p className="text-xs text-muted-foreground">
									Last daily claim: {formatRelativeTime(account.last_daily_at)}
								</p>
							</div>
						) : null}
					</QueryBoundary>
				)}

				{/* Offered even when no account row exists: adjusting creates it, which
				    is exactly what an admin wants for a member who has never earned. */}
				{submitted !== null ? (
					<div className="flex justify-end">
						<PermissionButton
							allowed={canWrite}
							reason={deniedReason}
							variant="ghost"
							size="sm"
							onClick={() => setAdjusting(true)}
						>
							<Plus className="size-4" />
							Adjust balance
						</PermissionButton>
					</div>
				) : null}

				{submitted !== null ? (
					<AdjustDialog
						open={adjusting}
						onOpenChange={setAdjusting}
						guildId={guildId}
						discordId={submitted}
					/>
				) : null}
			</CardContent>
		</Card>
	)
}

/**
 * Grants or deducts currency.
 *
 * The sign of the amount decides the direction, which is the API's contract — so
 * the form offers both and sends what the admin typed rather than inventing a
 * second field. The reason is required because the API writes it into the ledger,
 * and an unexplained adjustment is indistinguishable from a bug later.
 */
function AdjustDialog({
	open,
	onOpenChange,
	guildId,
	discordId,
}: {
	open: boolean
	onOpenChange: (open: boolean) => void
	guildId: string
	discordId: string
}) {
	const queryClient = useQueryClient()
	const [amount, setAmount] = useState("")
	const [reason, setReason] = useState("")

	const parsed = Number.parseInt(amount, 10)
	const valid = Number.isFinite(parsed) && parsed !== 0 && reason.trim().length > 0

	const adjust = useMutation({
		mutationFn: (body: { amount: number; reason: string }) =>
			economyApi.adjust(guildId, discordId, body),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: queryKeys.economy.all(guildId) })
			onOpenChange(false)
			setAmount("")
			setReason("")
			toast.success("Balance adjusted")
		},
	})

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>Adjust balance</DialogTitle>
					<DialogDescription>
						A positive amount grants, a negative amount deducts. The entry is recorded in the ledger
						with your account as the actor.
					</DialogDescription>
				</DialogHeader>

				<div className="flex flex-col gap-4">
					<FormField label="Amount" htmlFor="adjust-amount" description="Negative to deduct.">
						<Input
							id="adjust-amount"
							value={amount}
							onChange={(event) => setAmount(event.target.value)}
							placeholder="500"
							inputMode="numeric"
							className="font-mono"
						/>
					</FormField>
					<FormField label="Reason" htmlFor="adjust-reason">
						<Input
							id="adjust-reason"
							value={reason}
							onChange={(event) => setReason(event.target.value)}
							placeholder="Event prize"
						/>
					</FormField>
					{adjust.error ? (
						<p className="text-xs text-destructive">{normalizeApiError(adjust.error).message}</p>
					) : null}
				</div>

				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						Cancel
					</Button>
					<Button
						disabled={!valid || adjust.isPending}
						onClick={() => adjust.mutate({ amount: parsed, reason: reason.trim() })}
					>
						{parsed < 0 ? <Minus className="size-4" /> : <Plus className="size-4" />}
						Apply
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
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
