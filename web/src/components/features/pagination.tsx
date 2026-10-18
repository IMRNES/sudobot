import { ChevronLeft, ChevronRight } from "lucide-react"
import { Button } from "#/components/ui/button.tsx"
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "#/components/ui/select.tsx"
import { PAGE_SIZE_OPTIONS, paginationSummary, paginationWindow } from "#/hooks/use-pagination.ts"
import type { IPaginationMeta } from "#/libs/api/types.ts"
import { cn } from "#/libs/utils.ts"

/**
 * Pagination control driven by the API's `meta` block.
 *
 * The server owns the arithmetic (`total_pages`, `has_next`, `has_prev`), so
 * this component only renders what it is told — which keeps the client honest
 * when a filter changes the totals under it.
 */
export function Pagination({
	meta,
	onPageChange,
	onPerPageChange,
	className,
	disabled = false,
}: {
	meta: IPaginationMeta
	onPageChange: (page: number) => void
	onPerPageChange?: (perPage: number) => void
	className?: string
	disabled?: boolean
}) {
	const pages = paginationWindow(meta.page, Math.max(1, meta.total_pages))

	return (
		<nav
			aria-label="Pagination"
			className={cn(
				"flex flex-wrap items-center justify-between gap-4 px-1 py-3 text-sm",
				className,
			)}
		>
			<p className="text-muted-foreground">{paginationSummary(meta)}</p>

			<div className="flex items-center gap-4">
				{onPerPageChange ? (
					<div className="flex items-center gap-2">
						<span className="text-muted-foreground">Rows</span>
						<Select
							value={String(meta.per_page)}
							onValueChange={(value) => onPerPageChange(Number(value))}
							disabled={disabled}
						>
							<SelectTrigger size="sm" className="w-[4.5rem]" aria-label="Rows per page">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{PAGE_SIZE_OPTIONS.map((size) => (
									<SelectItem key={size} value={String(size)}>
										{size}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>
				) : null}

				<div className="flex items-center gap-1">
					<Button
						variant="outline"
						size="icon-sm"
						onClick={() => onPageChange(meta.page - 1)}
						disabled={disabled || !meta.has_prev}
						aria-label="Previous page"
					>
						<ChevronLeft />
					</Button>

					{pages.map((page, index) =>
						page === null ? (
							<span
								// biome-ignore lint/suspicious/noArrayIndexKey: ellipsis slots are positional by nature
								key={`gap-${index}`}
								className="px-1 text-muted-foreground"
								aria-hidden="true"
							>
								…
							</span>
						) : (
							<Button
								key={page}
								variant={page === meta.page ? "default" : "ghost"}
								size="icon-sm"
								onClick={() => onPageChange(page)}
								disabled={disabled}
								aria-label={`Page ${page}`}
								aria-current={page === meta.page ? "page" : undefined}
								className="tabular-nums"
							>
								{page}
							</Button>
						),
					)}

					<Button
						variant="outline"
						size="icon-sm"
						onClick={() => onPageChange(meta.page + 1)}
						disabled={disabled || !meta.has_next}
						aria-label="Next page"
					>
						<ChevronRight />
					</Button>
				</div>
			</div>
		</nav>
	)
}
