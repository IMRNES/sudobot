import { useQuery } from "@tanstack/react-query"
import { MoreHorizontal, Package, Pencil, Plus, Trash2 } from "lucide-react"
import { useState } from "react"
import { ConfirmDialog } from "#/components/features/confirm-dialog.tsx"
import { DataTable, type IDataTableColumn } from "#/components/features/data-table.tsx"
import { RoleName } from "#/components/features/discord-pickers.tsx"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { Pagination } from "#/components/features/pagination.tsx"
import { PermissionButton } from "#/components/features/permission-button.tsx"
import { TableSearch, TableToolbar } from "#/components/features/table-toolbar.tsx"
import { Badge } from "#/components/ui/badge.tsx"
import { Button } from "#/components/ui/button.tsx"
import { Card, CardContent, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "#/components/ui/dropdown-menu.tsx"
import { assignableRoles, useGuildDirectory } from "#/hooks/use-guild-directory.ts"
import { useTableSearch } from "#/hooks/use-table-search.ts"
import { economyApi } from "#/libs/api/resources.ts"
import type { IShopItemResponse } from "#/libs/api/types.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { formatNumber } from "#/libs/utils.ts"
import { ItemDialog } from "#/routes/_authenticated/$guildSlug/_components/item-dialog.tsx"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"
import { useInvalidatingMutation } from "#/routes/_authenticated/$guildSlug/_hooks/use-invalidating-mutation.ts"

/**
 * Shop items: list, create, edit, delete.
 *
 * Pagination follows the API contract exactly — `?page=&per_page=&search=` with
 * `meta` read from the response body — and the table state lives in the URL, so
 * a filtered view is shareable.
 */
export function ShopItemsPanel({
	guildId,
	routeId,
	canWrite,
	deniedReason,
}: {
	guildId: string
	/** Route id whose search params hold the table state. */
	routeId: string
	canWrite: boolean
	deniedReason: string
}) {
	const table = useTableSearch(routeId)
	const query = useQuery(guildQueries.items(guildId, table.params))
	const items = query.data?.data ?? []
	const meta = query.data?.meta

	// Role names for the table and the item dialog. A shop item can award a role,
	// and a short id in the table does not say which role that is.
	const { roles, rolesById } = useGuildDirectory(guildId)

	const [dialogOpen, setDialogOpen] = useState(false)
	const [editing, setEditing] = useState<IShopItemResponse | undefined>(undefined)
	const [deleting, setDeleting] = useState<IShopItemResponse | null>(null)

	// One invalidation target covers create, edit and delete: they all change
	// the same paginated item list.
	const itemListKey = queryKeys.economy.itemLists(guildId)

	const createMutation = useInvalidatingMutation({
		mutationFn: (payload: Parameters<typeof economyApi.createItem>[1]) =>
			economyApi.createItem(guildId, payload),
		invalidateKeys: [itemListKey],
		successMessage: "Shop item created",
		errorMessage: "Could not create that item",
		onSuccess: () => setDialogOpen(false),
	})

	const updateMutation = useInvalidatingMutation({
		mutationFn: (payload: { id: string; body: Parameters<typeof economyApi.updateItem>[2] }) =>
			economyApi.updateItem(guildId, payload.id, payload.body),
		invalidateKeys: [itemListKey],
		successMessage: "Shop item saved",
		errorMessage: "Could not save that item",
		onSuccess: () => {
			setDialogOpen(false)
			setEditing(undefined)
		},
	})

	const deleteMutation = useInvalidatingMutation({
		mutationFn: (itemId: string) => economyApi.deleteItem(guildId, itemId),
		invalidateKeys: [itemListKey],
		successMessage: "Shop item deleted",
		errorMessage: "Could not delete that item",
		onSuccess: () => setDeleting(null),
	})

	const columns: IDataTableColumn<IShopItemResponse>[] = [
		{
			id: "name",
			header: "Item",
			sortBy: "name",
			cell: (item) => (
				<div className="flex min-w-0 flex-col">
					<span className="truncate text-sm font-medium">{item.name}</span>
					<span className="truncate font-mono text-xs text-muted-foreground">{item.slug}</span>
				</div>
			),
		},
		{
			id: "price",
			header: "Price",
			align: "right",
			sortBy: "price",
			cell: (item) => <span className="tabular-nums">{formatNumber(item.price)}</span>,
		},
		{
			id: "stock",
			header: "Stock",
			align: "right",
			sortBy: "stock",
			cell: (item) =>
				item.stock === null ? (
					<span className="text-muted-foreground">Unlimited</span>
				) : (
					<span className="tabular-nums">{formatNumber(item.stock)}</span>
				),
		},
		{
			id: "role",
			header: "Role",
			cell: (item) =>
				item.role_id === null ? (
					<span className="text-muted-foreground">—</span>
				) : (
					<RoleName roleId={item.role_id} rolesById={rolesById} />
				),
		},
		{
			id: "status",
			header: "Status",
			cell: (item) => (
				<Badge variant={item.is_active ? "default" : "outline"} className="font-normal">
					{item.is_active ? "Active" : "Hidden"}
				</Badge>
			),
		},
		{
			id: "actions",
			header: <span className="sr-only">Actions</span>,
			align: "right",
			cell: (item) => (
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button variant="ghost" size="icon-sm" aria-label={`Actions for ${item.name}`}>
							<MoreHorizontal />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end">
						<DropdownMenuItem
							disabled={!canWrite}
							onSelect={() => {
								setEditing(item)
								setDialogOpen(true)
							}}
						>
							<Pencil className="size-4" />
							Edit
						</DropdownMenuItem>
						<DropdownMenuItem
							variant="destructive"
							disabled={!canWrite}
							onSelect={() => setDeleting(item)}
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
					<CardTitle className="text-base">Shop items</CardTitle>
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
						New item
					</PermissionButton>
				</div>
				<TableToolbar>
					<TableSearch
						value={table.search}
						onChange={table.setSearch}
						placeholder="Search items…"
					/>
					{meta ? (
						<span className="text-xs text-muted-foreground">
							{formatNumber(meta.total)} {meta.total === 1 ? "item" : "items"}
						</span>
					) : null}
				</TableToolbar>
			</CardHeader>

			<CardContent className="flex flex-col gap-2 px-0">
				<DataTable
					columns={columns}
					rows={items}
					getRowId={(item) => item.id}
					isLoading={query.isLoading}
					sortBy={table.sortBy}
					sortDirection={table.sortDirection}
					onSort={table.setSort}
					className="rounded-none border-x-0"
					emptyState={
						<EmptyState
							icon={Package}
							title="No shop items yet"
							description="Create the first one so members have something to spend their balance on."
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
									Create the first item
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

			<ItemDialog
				open={dialogOpen}
				onOpenChange={(open) => {
					setDialogOpen(open)
					if (!open) setEditing(undefined)
				}}
				mode={editing ? "edit" : "create"}
				item={editing}
				isPending={createMutation.isPending || updateMutation.isPending}
				submitError={createMutation.error ?? updateMutation.error}
				roles={assignableRoles(roles)}
				onSubmit={(payload) => {
					if (editing) updateMutation.mutate({ id: editing.id, body: payload })
					else createMutation.mutate(payload)
				}}
			/>

			<ConfirmDialog
				open={deleting !== null}
				onOpenChange={(open) => {
					if (!open) setDeleting(null)
				}}
				title="Delete shop item"
				description={
					<>
						<span className="font-medium text-foreground">{deleting?.name}</span> will be removed
						from the shop. Members who already bought it keep the role it granted.
					</>
				}
				confirmLabel="Delete item"
				isPending={deleteMutation.isPending}
				onConfirm={() => {
					if (deleting) deleteMutation.mutate(deleting.id)
				}}
			/>
		</Card>
	)
}
