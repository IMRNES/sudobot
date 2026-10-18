import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { MoreHorizontal, Pencil, Plus, Trash2, UserPlus } from "lucide-react"
import { useState } from "react"
import { toast } from "sonner"
import { ConfirmDialog } from "#/components/features/confirm-dialog.tsx"
import { DataTable, type IDataTableColumn } from "#/components/features/data-table.tsx"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { FormField, FormRow } from "#/components/features/form-field.tsx"
import { Pagination } from "#/components/features/pagination.tsx"
import { PermissionButton } from "#/components/features/permission-button.tsx"
import { FormError } from "#/components/features/query-boundary.tsx"
import { TableSearch, TableToolbar } from "#/components/features/table-toolbar.tsx"
import { Badge } from "#/components/ui/badge.tsx"
import { Button } from "#/components/ui/button.tsx"
import { Card, CardContent, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "#/components/ui/dialog.tsx"
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "#/components/ui/dropdown-menu.tsx"
import { Input } from "#/components/ui/input.tsx"
import { Label } from "#/components/ui/label.tsx"
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "#/components/ui/select.tsx"
import { Switch } from "#/components/ui/switch.tsx"
import { useTableSearch } from "#/hooks/use-table-search.ts"
import { autorolesApi } from "#/libs/api/resources.ts"
import type {
	IAutoRoleCreateRequest,
	IAutoRoleResponse,
	TAutoRoleTrigger,
} from "#/libs/api/types.ts"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { formatDuration, formatNumber } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/** The three triggers the API accepts, with plain-language copy. */
const TRIGGERS: { value: TAutoRoleTrigger; label: string; help: string }[] = [
	{ value: "join", label: "On join", help: "Immediately, before the member is verified." },
	{ value: "delay", label: "After a delay", help: "Once the member has been here for a while." },
	{
		value: "human",
		label: "After verification",
		help: "Once Discord confirms they are not a bot.",
	},
]

const TRIGGER_LABEL = new Map(TRIGGERS.map((trigger) => [trigger.value, trigger.label]))

/**
 * Autorole rules: list, create, edit and delete.
 *
 * `delay_seconds` only means anything for the `delay` trigger, so the field
 * appears only when that trigger is chosen rather than sitting there disabled —
 * the API bounds it to 0–604800 and the form enforces the same range.
 */
export function AutorolesPanel({
	guildId,
	routeId,
	canWrite,
	deniedReason,
}: {
	guildId: string
	routeId: string
	canWrite: boolean
	deniedReason: string
}) {
	const table = useTableSearch(routeId)
	const queryClient = useQueryClient()
	const query = useQuery(guildQueries.autoroles(guildId, table.params))
	const autoroles = query.data?.data ?? []
	const meta = query.data?.meta

	const [dialogOpen, setDialogOpen] = useState(false)
	const [editing, setEditing] = useState<IAutoRoleResponse | undefined>(undefined)
	const [deleting, setDeleting] = useState<IAutoRoleResponse | null>(null)

	const invalidate = () => queryKeys.autoroles.lists(guildId)

	const createMutation = useMutation({
		mutationFn: (body: IAutoRoleCreateRequest) => autorolesApi.create(guildId, body),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: invalidate() })
			setDialogOpen(false)
			toast.success("Autorole created")
		},
	})

	const updateMutation = useMutation({
		mutationFn: (payload: { id: string; body: Parameters<typeof autorolesApi.update>[2] }) =>
			autorolesApi.update(guildId, payload.id, payload.body),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: invalidate() })
			setDialogOpen(false)
			setEditing(undefined)
			toast.success("Autorole updated")
		},
	})

	const deleteMutation = useMutation({
		mutationFn: (id: string) => autorolesApi.remove(guildId, id),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: invalidate() })
			setDeleting(null)
			toast.success("Autorole deleted")
		},
	})

	const toggleActive = useMutation({
		mutationFn: (rule: IAutoRoleResponse) =>
			autorolesApi.update(guildId, rule.id, { is_active: !rule.is_active }),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: invalidate() })
		},
		onError: (error) => {
			const normalised = normalizeApiError(error)
			toast.error("Could not change that rule", { description: normalised.message })
		},
	})

	const columns: IDataTableColumn<IAutoRoleResponse>[] = [
		{
			id: "role",
			header: "Role id",
			cell: (rule) => <span className="font-mono text-xs">{rule.role_id}</span>,
		},
		{
			id: "trigger",
			header: "Trigger",
			sortBy: "trigger",
			cell: (rule) => (
				<div className="flex flex-col">
					<span className="text-sm">{TRIGGER_LABEL.get(rule.trigger) ?? rule.trigger}</span>
					{rule.trigger === "delay" ? (
						<span className="text-xs text-muted-foreground">
							after {formatDuration(rule.delay_seconds)}
						</span>
					) : null}
				</div>
			),
		},
		{
			id: "bots",
			header: "Applies to",
			cell: (rule) => (
				<span className="text-sm text-muted-foreground">
					{rule.applies_to_bots ? "Members and bots" : "Members only"}
				</span>
			),
		},
		{
			id: "active",
			header: "Active",
			cell: (rule) => (
				<Badge variant={rule.is_active ? "default" : "outline"} className="font-normal">
					{rule.is_active ? "Active" : "Paused"}
				</Badge>
			),
		},
		{
			id: "actions",
			header: <span className="sr-only">Actions</span>,
			align: "right",
			cell: (rule) => (
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button variant="ghost" size="icon-sm" aria-label={`Actions for role ${rule.role_id}`}>
							<MoreHorizontal />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end">
						<DropdownMenuItem disabled={!canWrite} onSelect={() => toggleActive.mutate(rule)}>
							{rule.is_active ? "Pause rule" : "Activate rule"}
						</DropdownMenuItem>
						<DropdownMenuItem
							disabled={!canWrite}
							onSelect={() => {
								setEditing(rule)
								setDialogOpen(true)
							}}
						>
							<Pencil className="size-4" />
							Edit
						</DropdownMenuItem>
						<DropdownMenuItem
							variant="destructive"
							disabled={!canWrite}
							onSelect={() => setDeleting(rule)}
						>
							<Trash2 className="size-4" />
							Delete
						</DropdownMenuItem>
					</DropdownMenuContent>
				</DropdownMenu>
			),
		},
	]

	return (
		<Card>
			<CardHeader className="gap-4">
				<div className="flex flex-wrap items-center justify-between gap-3">
					<CardTitle className="text-base">Rules</CardTitle>
					<PermissionButton
						allowed={canWrite}
						reason={deniedReason}
						size="sm"
						onClick={() => {
							setEditing(undefined)
							setDialogOpen(true)
						}}
					>
						<Plus />
						New rule
					</PermissionButton>
				</div>
				<TableToolbar>
					<TableSearch
						value={table.search}
						onChange={table.setSearch}
						placeholder="Search rules…"
					/>
					{meta ? (
						<span className="text-xs text-muted-foreground">
							{formatNumber(meta.total)} {meta.total === 1 ? "rule" : "rules"}
						</span>
					) : null}
				</TableToolbar>
			</CardHeader>

			<CardContent className="flex flex-col gap-2 px-0">
				<DataTable
					columns={columns}
					rows={autoroles}
					getRowId={(rule) => rule.id}
					isLoading={query.isLoading}
					sortBy={table.sortBy}
					sortDirection={table.sortDirection}
					onSort={table.setSort}
					className="rounded-none border-x-0"
					emptyState={
						<EmptyState
							icon={UserPlus}
							title="No autoroles yet"
							description="Add a rule so new members are given a role without anyone lifting a finger."
							action={
								<PermissionButton
									allowed={canWrite}
									reason={deniedReason}
									size="sm"
									onClick={() => {
										setEditing(undefined)
										setDialogOpen(true)
									}}
								>
									<Plus />
									Create the first rule
								</PermissionButton>
							}
						/>
					}
				/>

				{meta ? (
					<Pagination
						meta={meta}
						onPageChange={table.setPage}
						onPerPageChange={table.setPerPage}
						className="px-6"
					/>
				) : null}
			</CardContent>

			<AutoroleDialog
				open={dialogOpen}
				onOpenChange={(open) => {
					setDialogOpen(open)
					if (!open) setEditing(undefined)
				}}
				rule={editing}
				isPending={createMutation.isPending || updateMutation.isPending}
				submitError={createMutation.error ?? updateMutation.error}
				onSubmit={(payload) => {
					if (editing) {
						updateMutation.mutate({
							id: editing.id,
							body: {
								role_id: payload.role_id,
								trigger: payload.trigger,
								delay_seconds: payload.delay_seconds,
								applies_to_bots: payload.applies_to_bots,
							},
						})
					} else {
						createMutation.mutate(payload)
					}
				}}
			/>

			<ConfirmDialog
				open={deleting !== null}
				onOpenChange={(open) => {
					if (!open) setDeleting(null)
				}}
				title="Delete autorole rule"
				description={
					<>
						Role <span className="font-mono text-foreground">{deleting?.role_id}</span> will no
						longer be granted automatically. Members who already have it keep it.
					</>
				}
				confirmLabel="Delete rule"
				isPending={deleteMutation.isPending}
				onConfirm={() => {
					if (deleting) deleteMutation.mutate(deleting.id)
				}}
			/>
		</Card>
	)
}

interface IAutoroleFormValues {
	role_id: string
	trigger: TAutoRoleTrigger
	delay_seconds: string
	applies_to_bots: boolean
}

function AutoroleDialog({
	open,
	onOpenChange,
	rule,
	onSubmit,
	isPending,
	submitError,
}: {
	open: boolean
	onOpenChange: (open: boolean) => void
	rule?: IAutoRoleResponse
	onSubmit: (payload: IAutoRoleCreateRequest) => void
	isPending: boolean
	submitError: unknown
}) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>{rule ? "Edit autorole" : "New autorole"}</DialogTitle>
					<DialogDescription>
						A rule grants one role, once, when its trigger fires.
					</DialogDescription>
				</DialogHeader>
				{/* Keyed so switching between rules re-seeds the form. */}
				{open ? (
					<AutoroleForm
						key={rule?.id ?? "new"}
						rule={rule}
						onSubmit={onSubmit}
						onCancel={() => onOpenChange(false)}
						isPending={isPending}
						submitError={submitError}
					/>
				) : null}
			</DialogContent>
		</Dialog>
	)
}

function AutoroleForm({
	rule,
	onSubmit,
	onCancel,
	isPending,
	submitError,
}: {
	rule?: IAutoRoleResponse
	onSubmit: (payload: IAutoRoleCreateRequest) => void
	onCancel: () => void
	isPending: boolean
	submitError: unknown
}) {
	const [values, setValues] = useState<IAutoroleFormValues>({
		role_id: rule?.role_id ?? "",
		trigger: rule?.trigger ?? "join",
		delay_seconds: String(rule?.delay_seconds ?? 600),
		applies_to_bots: rule?.applies_to_bots ?? false,
	})
	const [roleError, setRoleError] = useState<string | undefined>(undefined)
	const [delayError, setDelayError] = useState<string | undefined>(undefined)

	const triggerHelp = TRIGGERS.find((entry) => entry.value === values.trigger)?.help

	const submit = () => {
		if (!/^\d{17,20}$/.test(values.role_id.trim())) {
			setRoleError("A role id is 17 to 20 digits.")
			return
		}
		const delay = Number.parseInt(values.delay_seconds, 10)
		if (values.trigger === "delay" && (!Number.isFinite(delay) || delay < 0 || delay > 604_800)) {
			setDelayError("Between 0 seconds and 7 days (604800).")
			return
		}
		setRoleError(undefined)
		setDelayError(undefined)
		onSubmit({
			role_id: values.role_id.trim(),
			trigger: values.trigger,
			// The API bounds this to 0-604800 and expects it regardless of trigger.
			delay_seconds: values.trigger === "delay" ? delay : 0,
			applies_to_bots: values.applies_to_bots,
		})
	}

	return (
		<form
			className="flex flex-col gap-4"
			onSubmit={(event) => {
				event.preventDefault()
				submit()
			}}
		>
			<FormField label="Role id" htmlFor="autorole-role" required error={roleError}>
				<Input
					id="autorole-role"
					value={values.role_id}
					onChange={(event) => {
						setValues((v) => ({ ...v, role_id: event.target.value }))
						setRoleError(undefined)
					}}
					placeholder="456789012345678901"
					inputMode="numeric"
					className="font-mono"
				/>
			</FormField>

			<FormField label="Trigger" htmlFor="autorole-trigger" description={triggerHelp}>
				<Select
					value={values.trigger}
					onValueChange={(value) =>
						setValues((v) => ({ ...v, trigger: value as TAutoRoleTrigger }))
					}
				>
					<SelectTrigger id="autorole-trigger" className="w-full">
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						{TRIGGERS.map((trigger) => (
							<SelectItem key={trigger.value} value={trigger.value}>
								{trigger.label}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</FormField>

			{values.trigger === "delay" ? (
				<FormRow>
					<FormField
						label="Delay"
						htmlFor="autorole-delay"
						required
						error={delayError}
						description={`${formatDuration(Number.parseInt(values.delay_seconds, 10) || 0)} after joining.`}
					>
						<Input
							id="autorole-delay"
							type="number"
							min={0}
							max={604_800}
							value={values.delay_seconds}
							onChange={(event) => {
								setValues((v) => ({ ...v, delay_seconds: event.target.value }))
								setDelayError(undefined)
							}}
							className="tabular-nums"
						/>
					</FormField>
				</FormRow>
			) : null}

			<div className="flex items-center justify-between gap-4 rounded-lg border border-border px-4 py-3">
				<div className="flex flex-col gap-0.5">
					<Label htmlFor="autorole-bots" className="cursor-pointer">
						Include bots
					</Label>
					<p className="text-xs text-muted-foreground">Grant the role to bots that join, too.</p>
				</div>
				<Switch
					id="autorole-bots"
					checked={values.applies_to_bots}
					onCheckedChange={(checked) => setValues((v) => ({ ...v, applies_to_bots: checked }))}
				/>
			</div>

			<FormError error={submitError} />

			<DialogFooter>
				<Button type="button" variant="outline" onClick={onCancel}>
					Cancel
				</Button>
				<Button type="submit" disabled={isPending}>
					{isPending ? "Saving…" : rule ? "Save changes" : "Create rule"}
				</Button>
			</DialogFooter>
		</form>
	)
}
