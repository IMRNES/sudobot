import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Plus, RotateCcw, Save, ShieldCheck, Trash2 } from "lucide-react"
import { useEffect, useMemo, useState } from "react"
import { toast } from "sonner"
import { ConfirmDialog } from "#/components/features/confirm-dialog.tsx"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { FormField } from "#/components/features/form-field.tsx"
import { PermissionButton } from "#/components/features/permission-button.tsx"
import { FormError, QueryBoundary } from "#/components/features/query-boundary.tsx"
import { Badge } from "#/components/ui/badge.tsx"
import { Button } from "#/components/ui/button.tsx"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { Checkbox } from "#/components/ui/checkbox.tsx"
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "#/components/ui/dialog.tsx"
import { Input } from "#/components/ui/input.tsx"
import { Skeleton } from "#/components/ui/skeleton.tsx"
import { Textarea } from "#/components/ui/textarea.tsx"
import { rolesApi } from "#/libs/api/resources.ts"
import type { IPermissionResponse, IRoleCreateRequest, IRoleResponse } from "#/libs/api/types.ts"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { cn, formatNumber } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * The permission matrix: a checkbox grid of role x permission.
 *
 * The whole grid is one form with one Save button, because `PUT
 * /roles/{roleId}/permissions` *replaces* the entire matrix — an optimistic
 * per-checkbox write would race against itself and leave the role in whichever
 * state the last request happened to carry. So edits are local, dirty state is
 * tracked per role, and nothing reaches the server until Save is pressed.
 *
 * `is_system` roles are shown but their checkboxes are locked: the API marks
 * them as built-in, and letting an admin empty out "Admin" would be a
 * foot-gun with no undo.
 */

export function PermissionMatrix({
	guildId,
	canWrite,
	deniedReason,
}: {
	guildId: string
	canWrite: boolean
	deniedReason: string
}) {
	const rolesQuery = useQuery(guildQueries.roles(guildId, { per_page: 100 }))
	const permissionsQuery = useQuery(guildQueries.permissions(guildId))

	return (
		<Card>
			<CardHeader>
				<CardTitle className="text-base">Permission matrix</CardTitle>
				<CardDescription>
					Every dashboard permission, by role. Changes are saved per role — nothing is written until
					you press Save on that role&rsquo;s row.
				</CardDescription>
			</CardHeader>
			<CardContent>
				<QueryBoundary
					isLoading={rolesQuery.isLoading || permissionsQuery.isLoading}
					isError={rolesQuery.isError || permissionsQuery.isError}
					error={rolesQuery.error ?? permissionsQuery.error}
					skeleton={<MatrixSkeleton />}
				>
					<MatrixGrid
						guildId={guildId}
						roles={rolesQuery.data?.data ?? []}
						permissions={permissionsQuery.data?.data ?? []}
						canWrite={canWrite}
						deniedReason={deniedReason}
					/>
				</QueryBoundary>
			</CardContent>
		</Card>
	)
}

function MatrixGrid({
	guildId,
	roles,
	permissions,
	canWrite,
	deniedReason,
}: {
	guildId: string
	roles: IRoleResponse[]
	permissions: IPermissionResponse[]
	canWrite: boolean
	deniedReason: string
}) {
	const queryClient = useQueryClient()

	/** Local, uncommitted matrix. Keyed by role id, then by permission name. */
	const [draft, setDraft] = useState<Record<string, Set<string>>>(() => seedDraft(roles))

	// Re-seed when the server data changes (a save lands, or another tab edits).
	// Dirty rows are preserved so an in-flight edit is not clobbered.
	useEffect(() => {
		setDraft((current) => mergeSeed(current, roles))
	}, [roles])

	const grouped = useMemo(() => groupByPermissionGroup(permissions), [permissions])

	const save = useMutation({
		mutationFn: (payload: { roleId: string; permissions: string[] }) =>
			rolesApi.setPermissions(guildId, payload.roleId, { permissions: payload.permissions }),
		onSuccess: (updated) => {
			// The PUT returns the new role, so patch it into every cached list
			// instead of refetching — the grid updates instantly.
			queryClient.setQueriesData<{ data: IRoleResponse[] }>(
				{ queryKey: queryKeys.roles.list(guildId) },
				(previous) =>
					previous === undefined
						? previous
						: {
								...previous,
								data: previous.data.map((role) => (role.id === updated.id ? updated : role)),
							},
			)
			toast.success(`${updated.name} saved`, {
				description: `${updated.permissions.length} permissions granted.`,
			})
		},
		onError: (error) => {
			const normalised = normalizeApiError(error)
			toast.error("Could not save permissions", { description: normalised.message })
		},
	})

	if (roles.length === 0) {
		return (
			<EmptyState
				icon={ShieldCheck}
				title="No roles defined"
				description="Create a role first, then grant it permissions here."
			/>
		)
	}

	return (
		<div className="overflow-x-auto rounded-lg border border-border">
			<table className="w-full border-collapse text-sm">
				<caption className="sr-only">
					Dashboard permissions by role. Each role has a Save button.
				</caption>
				<thead>
					<tr className="border-b border-border bg-muted/40">
						<th
							scope="col"
							className="sticky left-0 z-10 min-w-[13rem] bg-muted/40 px-4 py-2.5 text-left font-medium"
						>
							Permission
						</th>
						{roles.map((role) => (
							<th key={role.id} scope="col" className="min-w-[9rem] px-3 py-2.5 text-center">
								<span className="flex flex-col items-center gap-0.5">
									<span className="font-medium whitespace-nowrap">{role.name}</span>
									<span className="flex items-center gap-1 text-[10px] font-normal text-muted-foreground">
										{role.is_system ? "system" : role.slug}
										<span aria-hidden="true">·</span>
										{formatNumber(role.member_count)} members
									</span>
								</span>
							</th>
						))}
					</tr>
				</thead>

				<tbody>
					{grouped.map(([group, rows]) => (
						<PermissionGroup
							key={group}
							group={group}
							rows={rows}
							roles={roles}
							draft={draft}
							canWrite={canWrite}
							deniedReason={deniedReason}
							onToggle={(roleId, permission) => {
								setDraft((current) => toggleInDraft(current, roleId, permission))
							}}
						/>
					))}
				</tbody>

				<tfoot>
					<tr className="border-t border-border bg-muted/25">
						<td className="sticky left-0 z-10 bg-muted/25 px-4 py-2.5 text-xs font-medium text-muted-foreground">
							Save changes
						</td>
						{roles.map((role) => {
							const dirty = isDirty(role, draft[role.id])
							const locked = !canWrite || role.is_system
							return (
								<td key={role.id} className="px-3 py-2.5 text-center">
									<div className="flex items-center justify-center gap-1">
										<PermissionButton
											allowed={canWrite && !role.is_system}
											reason={
												role.is_system
													? "System roles are managed by the server and cannot be edited."
													: deniedReason
											}
											size="xs"
											disabled={!dirty || save.isPending}
											onClick={() =>
												save.mutate({
													roleId: role.id,
													permissions: [...(draft[role.id] ?? new Set<string>())],
												})
											}
										>
											<Save />
											Save
										</PermissionButton>
										{dirty && !locked ? (
											<Button
												variant="ghost"
												size="icon-xs"
												aria-label={`Discard changes to ${role.name}`}
												onClick={() => {
													setDraft((current) => ({
														...current,
														[role.id]: new Set(role.permissions),
													}))
												}}
											>
												<RotateCcw />
											</Button>
										) : null}
									</div>
								</td>
							)
						})}
					</tr>
				</tfoot>
			</table>
		</div>
	)
}

function PermissionGroup({
	group,
	rows,
	roles,
	draft,
	canWrite,
	deniedReason,
	onToggle,
}: {
	group: string
	rows: IPermissionResponse[]
	roles: IRoleResponse[]
	draft: Record<string, Set<string>>
	canWrite: boolean
	deniedReason: string
	onToggle: (roleId: string, permission: string) => void
}) {
	return (
		<>
			<tr className="border-b border-border bg-muted/15">
				<th
					scope="colgroup"
					colSpan={roles.length + 1}
					className="px-4 py-1.5 text-left text-[11px] font-semibold tracking-wider text-muted-foreground uppercase"
				>
					{group}
				</th>
			</tr>
			{rows.map((permission) => (
				<tr key={permission.id} className="border-b border-border last:border-b-0">
					<th scope="row" className="sticky left-0 z-10 bg-card px-4 py-2 text-left font-normal">
						<span className="flex flex-col gap-0.5">
							<span className="font-mono text-xs">{permission.name}</span>
							<span className="text-xs text-muted-foreground">{permission.description}</span>
						</span>
					</th>
					{roles.map((role) => {
						const checked = draft[role.id]?.has(permission.name) ?? false
						const locked = !canWrite || role.is_system
						return (
							<td key={role.id} className="px-3 py-2 text-center">
								<span
									className={cn("inline-flex", locked && "cursor-not-allowed opacity-60")}
									title={
										role.is_system
											? "System roles are managed by the server and cannot be edited."
											: canWrite
												? undefined
												: deniedReason
									}
								>
									<Checkbox
										checked={checked}
										disabled={locked}
										onCheckedChange={() => onToggle(role.id, permission.name)}
										aria-label={`${permission.name} for ${role.name}`}
									/>
								</span>
							</td>
						)
					})}
				</tr>
			))}
		</>
	)
}

/**
 * Role list management: create and delete. `PATCH /roles/{id}` exists but the
 * UI only exposes create/delete today, which keeps the page focused on the
 * matrix that is the point of it.
 */
export function RolesPanel({
	guildId,
	canWrite,
	deniedReason,
}: {
	guildId: string
	canWrite: boolean
	deniedReason: string
}) {
	const queryClient = useQueryClient()
	const query = useQuery(guildQueries.roles(guildId, { per_page: 100 }))
	const roles = query.data?.data ?? []

	const [dialogOpen, setDialogOpen] = useState(false)
	const [deleting, setDeleting] = useState<IRoleResponse | null>(null)
	// Deleting a role that still has members requires typing its slug, so the
	// typed text has to live alongside the dialog state that reads it.
	const [confirmText, setConfirmText] = useState("")

	const createMutation = useMutation({
		mutationFn: (body: IRoleCreateRequest) => rolesApi.create(guildId, body),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: queryKeys.roles.all(guildId) })
			setDialogOpen(false)
			toast.success("Role created")
		},
	})

	const deleteMutation = useMutation({
		mutationFn: (roleId: string) => rolesApi.remove(guildId, roleId),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: queryKeys.roles.all(guildId) })
			setDeleting(null)
			toast.success("Role deleted")
		},
	})

	return (
		<Card>
			<CardHeader className="flex-row items-start justify-between gap-4">
				<div className="flex flex-col gap-1">
					<CardTitle className="text-base">Roles</CardTitle>
					<CardDescription>The dashboard roles available in this server.</CardDescription>
				</div>
				<PermissionButton
					allowed={canWrite}
					reason={deniedReason}
					size="sm"
					onClick={() => setDialogOpen(true)}
				>
					<Plus />
					New role
				</PermissionButton>
			</CardHeader>
			<CardContent className="px-0 pb-0">
				<QueryBoundary isLoading={query.isLoading} isError={query.isError} error={query.error}>
					{roles.length === 0 ? (
						<EmptyState
							icon={ShieldCheck}
							title="No roles yet"
							description="Create a role to decide what a group of members can do."
						/>
					) : (
						<ul className="divide-y divide-border border-t border-border">
							{roles.map((role) => (
								<li key={role.id} className="flex items-center gap-4 px-6 py-3">
									<div className="min-w-0 flex-1">
										<div className="flex items-center gap-2">
											<span className="truncate text-sm font-medium">{role.name}</span>
											{role.is_system ? (
												<Badge variant="outline" className="font-normal">
													system
												</Badge>
											) : null}
										</div>
										<p className="truncate text-xs text-muted-foreground">
											{role.description ?? role.slug}
										</p>
									</div>
									<span className="shrink-0 text-xs text-muted-foreground tabular-nums">
										{role.permissions.length} perms · {formatNumber(role.member_count)} members
									</span>
									<Button
										variant="ghost"
										size="icon-sm"
										disabled={!canWrite || role.is_system}
										title={
											role.is_system
												? "System roles cannot be deleted."
												: canWrite
													? undefined
													: deniedReason
										}
										onClick={() => setDeleting(role)}
										aria-label={`Delete ${role.name}`}
									>
										<Trash2 />
									</Button>
								</li>
							))}
						</ul>
					)}
				</QueryBoundary>
			</CardContent>

			<CreateRoleDialog
				open={dialogOpen}
				onOpenChange={setDialogOpen}
				isPending={createMutation.isPending}
				submitError={createMutation.error}
				onSubmit={(body) => createMutation.mutate(body)}
			/>

			<ConfirmDialog
				open={deleting !== null}
				onOpenChange={(open) => {
					if (!open) {
						setDeleting(null)
						setConfirmText("")
					}
				}}
				title="Delete role"
				description={
					<>
						<span className="font-medium text-foreground">{deleting?.name}</span> will be removed.{" "}
						{deleting !== null && deleting.member_count > 0
							? `${formatNumber(deleting.member_count)} members hold this role and will lose the access it grants.`
							: "No members currently hold this role."}
					</>
				}
				confirmLabel="Delete role"
				isPending={deleteMutation.isPending}
				// Deleting a role that still has members is the dangerous case.
				requireConfirmation={
					deleting !== null && deleting.member_count > 0 ? deleting.slug : undefined
				}
				typedValue={confirmText}
				onTypedValueChange={setConfirmText}
				onConfirm={() => {
					if (deleting) deleteMutation.mutate(deleting.id)
				}}
			/>
		</Card>
	)
}

function CreateRoleDialog({
	open,
	onOpenChange,
	onSubmit,
	isPending,
	submitError,
}: {
	open: boolean
	onOpenChange: (open: boolean) => void
	onSubmit: (body: IRoleCreateRequest) => void
	isPending: boolean
	submitError: unknown
}) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>New role</DialogTitle>
					<DialogDescription>
						Roles group permissions. Nobody is assigned a new role automatically.
					</DialogDescription>
				</DialogHeader>
				{open ? (
					<CreateRoleForm
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

function CreateRoleForm({
	onSubmit,
	onCancel,
	isPending,
	submitError,
}: {
	onSubmit: (body: IRoleCreateRequest) => void
	onCancel: () => void
	isPending: boolean
	submitError: unknown
}) {
	const [values, setValues] = useState({ slug: "", name: "", description: "", position: "0" })
	const [slugError, setSlugError] = useState<string | undefined>(undefined)

	return (
		<form
			className="flex flex-col gap-4"
			onSubmit={(event) => {
				event.preventDefault()
				if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(values.slug.trim())) {
					setSlugError("Lowercase letters, numbers and hyphens, e.g. event-host.")
					return
				}
				if (values.name.trim().length === 0) return
				onSubmit({
					slug: values.slug.trim(),
					name: values.name.trim(),
					description: values.description.trim().length > 0 ? values.description.trim() : null,
					position: Number.parseInt(values.position, 10) || 0,
				})
			}}
		>
			<FormField label="Name" htmlFor="role-name" required>
				<Input
					id="role-name"
					value={values.name}
					onChange={(event) => setValues((v) => ({ ...v, name: event.target.value }))}
					placeholder="Event host"
				/>
			</FormField>

			<FormField
				label="Slug"
				htmlFor="role-slug"
				required
				error={slugError}
				description="Stable identifier shown in the dashboard."
			>
				<Input
					id="role-slug"
					value={values.slug}
					onChange={(event) => {
						setValues((v) => ({ ...v, slug: event.target.value }))
						setSlugError(undefined)
					}}
					placeholder="event-host"
					className="font-mono"
				/>
			</FormField>

			<FormField label="Description" htmlFor="role-description">
				<Textarea
					id="role-description"
					value={values.description}
					onChange={(event) => setValues((v) => ({ ...v, description: event.target.value }))}
					placeholder="Runs giveaways and community events."
					rows={2}
				/>
			</FormField>

			<FormField
				label="Position"
				htmlFor="role-position"
				description="Higher positions sort first. Display only."
			>
				<Input
					id="role-position"
					type="number"
					value={values.position}
					onChange={(event) => setValues((v) => ({ ...v, position: event.target.value }))}
					className="tabular-nums"
				/>
			</FormField>

			<FormError error={submitError} />

			<DialogFooter>
				<Button type="button" variant="outline" onClick={onCancel}>
					Cancel
				</Button>
				<Button type="submit" disabled={isPending}>
					{isPending ? "Creating…" : "Create role"}
				</Button>
			</DialogFooter>
		</form>
	)
}

/* -------------------------------------------------------------------------- */
/* Matrix helpers                                                              */
/* -------------------------------------------------------------------------- */

function seedDraft(roles: IRoleResponse[]): Record<string, Set<string>> {
	const draft: Record<string, Set<string>> = {}
	for (const role of roles) draft[role.id] = new Set(role.permissions)
	return draft
}

/** Keep uncommitted edits, adopt server values for every other role. */
function mergeSeed(
	current: Record<string, Set<string>>,
	roles: IRoleResponse[],
): Record<string, Set<string>> {
	const next: Record<string, Set<string>> = {}
	for (const role of roles) {
		const existing = current[role.id]
		next[role.id] =
			existing !== undefined && isDirty(role, existing) ? existing : new Set(role.permissions)
	}
	return next
}

function isDirty(role: IRoleResponse, current: Set<string> | undefined): boolean {
	if (current === undefined) return false
	if (current.size !== role.permissions.length) return true
	return role.permissions.some((permission) => !current.has(permission))
}

function toggleInDraft(
	draft: Record<string, Set<string>>,
	roleId: string,
	permission: string,
): Record<string, Set<string>> {
	const next = new Set(draft[roleId] ?? [])
	if (next.has(permission)) next.delete(permission)
	else next.add(permission)
	return { ...draft, [roleId]: next }
}

/** `[[groupName, permissions], ...]`, groups in first-seen order. */
function groupByPermissionGroup(
	permissions: IPermissionResponse[],
): [string, IPermissionResponse[]][] {
	const map = new Map<string, IPermissionResponse[]>()
	for (const permission of permissions) {
		const bucket = map.get(permission.group)
		if (bucket === undefined) map.set(permission.group, [permission])
		else bucket.push(permission)
	}
	return [...map.entries()]
}

function MatrixSkeleton() {
	return (
		<div className="flex flex-col gap-2 rounded-lg border border-border p-4" aria-busy="true">
			<Skeleton className="h-5 w-48" />
			<div className="flex flex-col gap-1.5">
				{Array.from({ length: 6 }, (_, index) => (
					// biome-ignore lint/suspicious/noArrayIndexKey: skeleton rows have no identity
					<Skeleton key={index} className="h-6 w-full" />
				))}
			</div>
		</div>
	)
}
