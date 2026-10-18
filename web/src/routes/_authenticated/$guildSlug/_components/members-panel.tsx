import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { MoreHorizontal, ShieldCheck, UserMinus, Users } from "lucide-react"
import { useState } from "react"
import { toast } from "sonner"
import { ConfirmDialog } from "#/components/features/confirm-dialog.tsx"
import { DataTable, type IDataTableColumn } from "#/components/features/data-table.tsx"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { Pagination } from "#/components/features/pagination.tsx"
import { TableSearch, TableToolbar } from "#/components/features/table-toolbar.tsx"
import { UserCell } from "#/components/features/user-cell.tsx"
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
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "#/components/ui/dropdown-menu.tsx"
import { Label } from "#/components/ui/label.tsx"
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "#/components/ui/select.tsx"
import { useTableSearch } from "#/hooks/use-table-search.ts"
import { membersApi } from "#/libs/api/resources.ts"
import type { IMemberResponse, IRoleResponse } from "#/libs/api/types.ts"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { formatNumber, formatRelativeTime } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * Member list with per-member dashboard-role assignment.
 *
 * The PATCH route changes which *dashboard* role a member holds, which is a
 * different axis from their Discord roles — the copy says so explicitly,
 * because conflating the two is the single most confusing thing about a
 * dashboard like this.
 */
export function MembersPanel({
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
	const query = useQuery(guildQueries.members(guildId, table.params))
	const rolesQuery = useQuery(guildQueries.roles(guildId, { per_page: 100 }))
	const members = query.data?.data ?? []
	const meta = query.data?.meta
	const roles = rolesQuery.data?.data ?? []

	const [editing, setEditing] = useState<IMemberResponse | null>(null)
	const [removing, setRemoving] = useState<IMemberResponse | null>(null)

	const invalidate = () => queryKeys.members.lists(guildId)

	const updateRole = useMutation({
		mutationFn: (payload: { userId: string; guildRoleId: string }) =>
			membersApi.updateRole(guildId, payload.userId, { guild_role_id: payload.guildRoleId }),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: invalidate() })
			setEditing(null)
			toast.success("Role updated")
		},
		onError: (error) => {
			const normalised = normalizeApiError(error)
			toast.error("Could not change that role", { description: normalised.message })
		},
	})

	const removeMember = useMutation({
		mutationFn: (userId: string) => membersApi.remove(guildId, userId),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: invalidate() })
			setRemoving(null)
			toast.success("Member removed from the dashboard")
		},
		onError: (error) => {
			const normalised = normalizeApiError(error)
			toast.error("Could not remove that member", { description: normalised.message })
		},
	})

	const columns: IDataTableColumn<IMemberResponse>[] = [
		{
			id: "member",
			header: "Member",
			sortBy: "username",
			cell: (member) => (
				<div className="flex items-center gap-2">
					<UserCell
						discordId={member.discord_id}
						name={member.global_name ?? member.username}
						avatar={member.avatar_url}
					/>
					{member.is_owner ? (
						<Badge variant="outline" className="font-normal">
							Owner
						</Badge>
					) : null}
				</div>
			),
		},
		{
			id: "role",
			header: "Dashboard role",
			sortBy: "role",
			cell: (member) =>
				member.role_name === null ? (
					<span className="text-sm text-muted-foreground">No role</span>
				) : (
					<div className="flex flex-col">
						<span className="text-sm">{member.role_name}</span>
						<span className="font-mono text-xs text-muted-foreground">{member.role_slug}</span>
					</div>
				),
		},
		{
			id: "synced",
			header: "Last synced",
			align: "right",
			cell: (member) => (
				<span className="text-xs text-muted-foreground">
					{formatRelativeTime(member.synced_at)}
				</span>
			),
		},
		{
			id: "actions",
			header: <span className="sr-only">Actions</span>,
			align: "right",
			cell: (member) => (
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button
							variant="ghost"
							size="icon-sm"
							aria-label={`Actions for ${member.global_name ?? member.username}`}
						>
							<MoreHorizontal />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end">
						<DropdownMenuLabel>Dashboard access</DropdownMenuLabel>
						<DropdownMenuSeparator />
						<DropdownMenuItem disabled={!canWrite} onSelect={() => setEditing(member)}>
							<ShieldCheck className="size-4" />
							Change role
						</DropdownMenuItem>
						<DropdownMenuItem
							variant="destructive"
							// The guild owner's row is the server's own admin entry; removing
							// it would leave nobody able to administer the dashboard.
							disabled={!canWrite || member.is_owner}
							onSelect={() => setRemoving(member)}
						>
							<UserMinus className="size-4" />
							Remove access
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
					<CardTitle className="text-base">Members</CardTitle>
					{meta ? (
						<span className="text-xs text-muted-foreground">
							{formatNumber(meta.total)} {meta.total === 1 ? "member" : "members"}
						</span>
					) : null}
				</div>
				<TableToolbar>
					<TableSearch
						value={table.search}
						onChange={table.setSearch}
						placeholder="Search by name…"
					/>
					{!canWrite ? <span className="text-xs text-muted-foreground">{deniedReason}</span> : null}
				</TableToolbar>
			</CardHeader>

			<CardContent className="flex flex-col gap-2 px-0">
				<DataTable
					columns={columns}
					rows={members}
					getRowId={(member) => member.user_id}
					isLoading={query.isLoading}
					sortBy={table.sortBy}
					sortDirection={table.sortDirection}
					onSort={table.setSort}
					className="rounded-none border-x-0"
					emptyState={
						<EmptyState
							icon={Users}
							title="No members match"
							description={
								table.search.length > 0
									? `Nothing matched “${table.search}”. Try a shorter search.`
									: "Members appear here once they sign in to the dashboard."
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

			<RoleChangeDialog
				member={editing}
				roles={roles}
				onOpenChange={(open) => {
					if (!open) setEditing(null)
				}}
				isPending={updateRole.isPending}
				onSubmit={(guildRoleId) => {
					if (editing) updateRole.mutate({ userId: editing.user_id, guildRoleId })
				}}
			/>

			<ConfirmDialog
				open={removing !== null}
				onOpenChange={(open) => {
					if (!open) setRemoving(null)
				}}
				title="Remove dashboard access"
				description={
					<>
						<span className="font-medium text-foreground">
							{removing?.global_name ?? removing?.username}
						</span>{" "}
						will lose access to this server&rsquo;s dashboard. Their Discord roles are untouched —
						this only affects sudobot.
					</>
				}
				confirmLabel="Remove access"
				isPending={removeMember.isPending}
				onConfirm={() => {
					if (removing) removeMember.mutate(removing.user_id)
				}}
			/>
		</Card>
	)
}

function RoleChangeDialog({
	member,
	roles,
	onOpenChange,
	onSubmit,
	isPending,
}: {
	member: IMemberResponse | null
	roles: IRoleResponse[]
	onOpenChange: (open: boolean) => void
	onSubmit: (guildRoleId: string) => void
	isPending: boolean
}) {
	return (
		<Dialog open={member !== null} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>Change dashboard role</DialogTitle>
					<DialogDescription>
						This role decides what{" "}
						<span className="font-medium text-foreground">
							{member?.global_name ?? member?.username}
						</span>{" "}
						can do in the sudobot dashboard. It is not a Discord role and changes nothing in the
						server itself.
					</DialogDescription>
				</DialogHeader>

				{/* Keyed on the member so a second open starts from that member's role. */}
				{member ? (
					<RoleSelectForm
						key={member.user_id}
						member={member}
						roles={roles}
						isPending={isPending}
						onSubmit={onSubmit}
						onCancel={() => onOpenChange(false)}
					/>
				) : null}
			</DialogContent>
		</Dialog>
	)
}

function RoleSelectForm({
	member,
	roles,
	onSubmit,
	isPending,
	onCancel,
}: {
	member: IMemberResponse
	roles: IRoleResponse[]
	isPending: boolean
	onSubmit: (guildRoleId: string) => void
	onCancel: () => void
}) {
	const [selected, setSelected] = useState(member.guild_role_id ?? "")

	return (
		<form
			className="flex flex-col gap-4"
			onSubmit={(event) => {
				event.preventDefault()
				if (selected.length > 0) onSubmit(selected)
			}}
		>
			<div className="flex flex-col gap-2">
				<Label htmlFor="member-role">Role</Label>
				<Select value={selected} onValueChange={setSelected}>
					<SelectTrigger id="member-role" className="w-full">
						<SelectValue placeholder="Pick a role" />
					</SelectTrigger>
					<SelectContent>
						{roles.map((role) => (
							<SelectItem key={role.id} value={role.id}>
								<span className="flex items-center gap-2">
									{role.name}
									{role.is_system ? (
										<Badge variant="outline" className="font-normal">
											system
										</Badge>
									) : null}
								</span>
							</SelectItem>
						))}
					</SelectContent>
				</Select>
				<p className="text-xs text-muted-foreground">
					{roles.length} {roles.length === 1 ? "role" : "roles"} defined for this server.
				</p>
			</div>

			<DialogFooter>
				<Button type="button" variant="outline" onClick={onCancel}>
					Cancel
				</Button>
				<Button type="submit" disabled={isPending || selected.length === 0}>
					{isPending ? "Saving…" : "Save role"}
				</Button>
			</DialogFooter>
		</form>
	)
}
