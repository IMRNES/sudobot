import { useForm } from "@tanstack/react-form"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Award, Plus, Trash2 } from "lucide-react"
import { useState } from "react"
import { ConfirmDialog } from "#/components/features/confirm-dialog.tsx"
import { DataTable, type IDataTableColumn } from "#/components/features/data-table.tsx"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { FormField, FormRow } from "#/components/features/form-field.tsx"
import { PermissionButton } from "#/components/features/permission-button.tsx"
import { FormError, QueryBoundary } from "#/components/features/query-boundary.tsx"
import { Badge } from "#/components/ui/badge.tsx"
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
import { Switch } from "#/components/ui/switch.tsx"
import { levelingApi } from "#/libs/api/resources.ts"
import type { IRewardCreateRequest, IRewardResponse } from "#/libs/api/types.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { formatNumber, shortId } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * Level rewards: the role a member earns at a given level.
 *
 * Rewards are append-only by design — the API offers `POST` and `DELETE` but no
 * `PATCH` — so the table has no edit affordance. Changing a reward means
 * deleting it and creating the replacement, which is honest about the contract
 * rather than pretending otherwise.
 */
export function RewardsPanel({
	guildId,
	canWrite,
	deniedReason,
}: {
	guildId: string
	canWrite: boolean
	deniedReason: string
}) {
	const queryClient = useQueryClient()
	const query = useQuery(guildQueries.rewards(guildId, { per_page: 100 }))
	const rewards = query.data?.data ?? []

	const [dialogOpen, setDialogOpen] = useState(false)
	const [deleting, setDeleting] = useState<IRewardResponse | null>(null)

	const createMutation = useMutation({
		mutationFn: (body: IRewardCreateRequest) => levelingApi.createReward(guildId, body),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: queryKeys.leveling.rewardLists(guildId) })
			setDialogOpen(false)
		},
	})

	const deleteMutation = useMutation({
		mutationFn: (rewardId: string) => levelingApi.deleteReward(guildId, rewardId),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: queryKeys.leveling.rewardLists(guildId) })
			setDeleting(null)
		},
	})

	const columns: IDataTableColumn<IRewardResponse>[] = [
		{
			id: "level",
			header: "Level",
			sortBy: "level",
			cell: (reward) => (
				<Badge variant="secondary" className="font-mono tabular-nums">
					{formatNumber(reward.level)}
				</Badge>
			),
		},
		{
			id: "role",
			header: "Role id",
			cell: (reward) => <span className="font-mono text-xs">{reward.role_id}</span>,
		},
		{
			id: "keep",
			header: "On demote",
			cell: (reward) => (
				<Badge variant={reward.keep_on_demote ? "default" : "outline"} className="font-normal">
					{reward.keep_on_demote ? "Kept" : "Removed"}
				</Badge>
			),
		},
		{
			id: "actions",
			header: <span className="sr-only">Actions</span>,
			align: "right",
			cell: (reward) => (
				<Button
					variant="ghost"
					size="icon-sm"
					disabled={!canWrite}
					title={canWrite ? undefined : deniedReason}
					onClick={() => setDeleting(reward)}
					aria-label={`Delete reward for level ${reward.level}`}
				>
					<Trash2 />
				</Button>
			),
		},
	]

	return (
		<Card>
			<CardHeader className="flex-row items-start justify-between gap-4">
				<div className="flex flex-col gap-1">
					<CardTitle className="text-base">Level rewards</CardTitle>
					<CardDescription>The role a member is granted on reaching each level.</CardDescription>
				</div>
				<PermissionButton
					allowed={canWrite}
					reason={deniedReason}
					size="sm"
					onClick={() => setDialogOpen(true)}
				>
					<Plus />
					Add reward
				</PermissionButton>
			</CardHeader>
			<CardContent className="px-0 pb-0">
				<QueryBoundary isLoading={query.isLoading} isError={query.isError} error={query.error}>
					<DataTable
						columns={columns}
						rows={rewards}
						getRowId={(reward) => reward.id}
						sortBy="level"
						sortDirection="asc"
						className="rounded-none border-x-0 border-b-0"
						emptyState={
							<EmptyState
								icon={Award}
								title="No rewards configured"
								description="Add your first reward so levelling has something to give."
								action={
									<PermissionButton
										allowed={canWrite}
										reason={deniedReason}
										size="sm"
										onClick={() => setDialogOpen(true)}
									>
										<Plus />
										Add the first reward
									</PermissionButton>
								}
							/>
						}
					/>
				</QueryBoundary>
			</CardContent>

			<RewardDialog
				open={dialogOpen}
				onOpenChange={setDialogOpen}
				isPending={createMutation.isPending}
				submitError={createMutation.error}
				onSubmit={(body) => createMutation.mutate(body)}
			/>

			<ConfirmDialog
				open={deleting !== null}
				onOpenChange={(open) => {
					if (!open) setDeleting(null)
				}}
				title="Delete level reward"
				description={
					<>
						Level <span className="font-medium text-foreground">{deleting?.level}</span> will no
						longer grant role{" "}
						<span className="font-mono text-foreground">{shortId(deleting?.role_id ?? "")}</span>.
						Members who already earned it keep it.
					</>
				}
				confirmLabel="Delete reward"
				isPending={deleteMutation.isPending}
				onConfirm={() => {
					if (deleting) deleteMutation.mutate(deleting.id)
				}}
			/>
		</Card>
	)
}

function RewardDialog({
	open,
	onOpenChange,
	onSubmit,
	isPending,
	submitError,
}: {
	open: boolean
	onOpenChange: (open: boolean) => void
	onSubmit: (body: IRewardCreateRequest) => void
	isPending: boolean
	submitError: unknown
}) {
	const form = useForm({
		defaultValues: { level: "", role_id: "", keep_on_demote: true },
		onSubmit: ({ value }) => {
			onSubmit({
				level: Number.parseInt(value.level, 10),
				role_id: value.role_id.trim(),
				keep_on_demote: value.keep_on_demote,
			})
		},
	})

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>Add level reward</DialogTitle>
					<DialogDescription>
						When a member reaches this level, sudobot grants the role you name here.
					</DialogDescription>
				</DialogHeader>

				<form
					className="flex flex-col gap-4"
					onSubmit={(event) => {
						event.preventDefault()
						event.stopPropagation()
						void form.handleSubmit()
					}}
				>
					<FormRow>
						<form.Field
							name="level"
							validators={{
								onChange: ({ value }) => {
									const parsed = Number.parseInt(value, 10)
									return Number.isFinite(parsed) && parsed >= 1 ? undefined : "Level 1 or higher."
								},
							}}
						>
							{(field) => (
								<FormField
									label="Level"
									htmlFor={field.name}
									required
									error={field.state.meta.errors[0]?.toString()}
								>
									<Input
										id={field.name}
										type="number"
										min={1}
										value={field.state.value}
										onBlur={field.handleBlur}
										onChange={(event) => field.handleChange(event.target.value)}
										placeholder="5"
										className="tabular-nums"
									/>
								</FormField>
							)}
						</form.Field>

						<form.Field
							name="role_id"
							validators={{
								onChange: ({ value }) =>
									/^\d{17,20}$/.test(value.trim()) ? undefined : "A role id is 17 to 20 digits.",
							}}
						>
							{(field) => (
								<FormField
									label="Role id"
									htmlFor={field.name}
									required
									error={field.state.meta.errors[0]?.toString()}
								>
									<Input
										id={field.name}
										value={field.state.value}
										onBlur={field.handleBlur}
										onChange={(event) => field.handleChange(event.target.value)}
										placeholder="456789012345678901"
										inputMode="numeric"
										className="font-mono"
									/>
								</FormField>
							)}
						</form.Field>
					</FormRow>

					<form.Field name="keep_on_demote">
						{(field) => (
							<div className="flex items-center justify-between gap-4 rounded-lg border border-border px-4 py-3">
								<div className="flex flex-col gap-0.5">
									<Label htmlFor={field.name} className="cursor-pointer">
										Keep on demote
									</Label>
									<p className="text-xs text-muted-foreground">
										Leave the role in place if the member&rsquo;s level later drops.
									</p>
								</div>
								<Switch
									id={field.name}
									checked={field.state.value}
									onCheckedChange={(checked) => field.handleChange(checked)}
								/>
							</div>
						)}
					</form.Field>

					<FormError error={submitError} />

					<DialogFooter>
						<Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
							Cancel
						</Button>
						<Button type="submit" disabled={isPending}>
							{isPending ? "Adding…" : "Add reward"}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	)
}
