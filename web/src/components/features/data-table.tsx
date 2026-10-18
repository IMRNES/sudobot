import { Skeleton } from "#/components/ui/skeleton.tsx"
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "#/components/ui/table.tsx"
import { cn } from "#/libs/utils.ts"

/**
 * The one table used by every list page.
 *
 * It owns the three states a list always has — loading, empty, loaded — so no
 * route has to re-implement them, and the empty state is passed in so each page
 * can say something specific ("No shop items yet — create the first one")
 * instead of leaving a blank rectangle.
 */

export interface IDataTableColumn<TItem> {
	/** Stable key; also the React key for the header cell. */
	id: string
	header: React.ReactNode
	/** Cell renderer. Receives the row so callers keep full control. */
	cell: (item: TItem) => React.ReactNode
	/** Tailwind class applied to both the header and the cells. */
	className?: string
	/** Set for numeric columns so they right-align. */
	align?: "left" | "right" | "center"
	/** Field name for the API's `sort_by`; renders a sort button when present. */
	sortBy?: string
}

export interface IDataTableProps<TItem> {
	columns: IDataTableColumn<TItem>[]
	rows: TItem[]
	/** Stable identity for React's reconciler — usually the row's `id`. */
	getRowId: (item: TItem) => string
	isLoading?: boolean
	/** Rendered in place of the table body when `rows` is empty. */
	emptyState?: React.ReactNode
	/** Current sort, for the header affordance. */
	sortBy?: string
	sortDirection?: "asc" | "desc"
	onSort?: (sortBy: string, direction: "asc" | "desc") => void
	className?: string
	/** Number of skeleton rows while loading. */
	skeletonRows?: number
}

const ALIGN_CLASS = {
	left: "text-left",
	right: "text-right",
	center: "text-center",
} as const

export function DataTable<TItem>({
	columns,
	rows,
	getRowId,
	isLoading = false,
	emptyState,
	sortBy,
	sortDirection,
	onSort,
	className,
	skeletonRows = 5,
}: IDataTableProps<TItem>) {
	// Loading wins over empty: an empty array mid-flight means "not yet known",
	// not "nothing here".
	if (isLoading) {
		return <DataTableSkeleton columns={columns.length} rows={skeletonRows} className={className} />
	}

	if (rows.length === 0) {
		return (
			<div className={cn("rounded-xl border border-border bg-card", className)}>
				{emptyState ?? <DefaultEmptyState />}
			</div>
		)
	}

	return (
		<div className={cn("overflow-hidden rounded-xl border border-border bg-card", className)}>
			<Table>
				<TableHeader>
					<TableRow className="hover:bg-transparent">
						{columns.map((column) => {
							const align = ALIGN_CLASS[column.align ?? "left"]
							const sortable = column.sortBy !== undefined && onSort !== undefined
							const isActive = sortable && sortBy === column.sortBy
							return (
								<TableHead
									key={column.id}
									className={cn(
										"px-4 text-xs font-medium text-muted-foreground",
										align,
										column.className,
									)}
									aria-sort={
										isActive ? (sortDirection === "desc" ? "descending" : "ascending") : undefined
									}
								>
									{sortable ? (
										<button
											type="button"
											onClick={() =>
												onSort(
													column.sortBy as string,
													isActive && sortDirection === "asc" ? "desc" : "asc",
												)
											}
											className={cn(
												"inline-flex items-center gap-1 rounded-sm transition-colors hover:text-foreground",
												isActive && "text-foreground",
												column.align === "right" && "flex-row-reverse",
											)}
										>
											{column.header}
											<span aria-hidden="true" className="text-[10px] leading-none">
												{isActive ? (sortDirection === "desc" ? "▼" : "▲") : "↕"}
											</span>
										</button>
									) : (
										column.header
									)}
								</TableHead>
							)
						})}
					</TableRow>
				</TableHeader>
				<TableBody>
					{rows.map((item) => (
						<TableRow key={getRowId(item)}>
							{columns.map((column) => (
								<TableCell
									key={column.id}
									className={cn("px-4 py-3", ALIGN_CLASS[column.align ?? "left"], column.className)}
								>
									{column.cell(item)}
								</TableCell>
							))}
						</TableRow>
					))}
				</TableBody>
			</Table>
		</div>
	)
}

function DataTableSkeleton({
	columns,
	rows,
	className,
}: {
	columns: number
	rows: number
	className?: string
}) {
	return (
		<div
			className={cn("overflow-hidden rounded-xl border border-border bg-card", className)}
			aria-busy="true"
		>
			<div className="border-b border-border px-4 py-3">
				<Skeleton className="h-4 w-40" />
			</div>
			<div className="flex flex-col">
				{Array.from({ length: rows }, (_, rowIndex) => (
					<div
						// biome-ignore lint/suspicious/noArrayIndexKey: skeleton rows have no identity
						key={rowIndex}
						className="flex items-center gap-4 border-b border-border px-4 py-3 last:border-b-0"
					>
						{Array.from({ length: Math.max(1, columns) }, (_, columnIndex) => (
							<Skeleton
								// biome-ignore lint/suspicious/noArrayIndexKey: skeleton cells have no identity
								key={columnIndex}
								className="h-4 flex-1"
								style={{ maxWidth: `${100 / Math.max(1, columns)}%` }}
							/>
						))}
					</div>
				))}
			</div>
		</div>
	)
}

function DefaultEmptyState() {
	return (
		<div className="flex flex-col items-center gap-1 px-6 py-12 text-center">
			<p className="text-sm font-medium">Nothing here yet</p>
			<p className="text-sm text-muted-foreground">
				Once there is data to show, it will appear in this table.
			</p>
		</div>
	)
}
