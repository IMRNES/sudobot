import type { IListParams } from "#/libs/api/types.ts"

/**
 * The subset of `IListParams` a table actually drives, with `undefined` for
 * "not set" so the values can be spread straight into a query key and a query
 * string without sentinel values leaking into either.
 */
export type TListParamsState = Pick<
	IListParams,
	"page" | "per_page" | "search" | "sort_by" | "sort_direction"
>

export const PAGE_SIZE_OPTIONS = [10, 20, 50, 100] as const

/** Default page size, mirroring the API's own default. */
export const DEFAULT_PER_PAGE = 20

export type { IPaginationMeta } from "#/libs/api/types.ts"

/**
 * Window of page numbers to render, with `null` marking an ellipsis.
 *
 * A guild's member list can run to hundreds of pages, so the control shows the
 * first page, the last page and a short window around the current one.
 */
export function paginationWindow(current: number, total: number, siblings = 1): (number | null)[] {
	const totalSlots = siblings * 2 + 5
	if (total <= totalSlots) {
		return Array.from({ length: total }, (_, index) => index + 1)
	}

	const left = Math.max(current - siblings, 2)
	const right = Math.min(current + siblings, total - 1)
	const showLeftEllipsis = left > 2
	const showRightEllipsis = right < total - 1

	const pages: (number | null)[] = [1]
	if (showLeftEllipsis) pages.push(null)
	for (let page = left; page <= right; page += 1) pages.push(page)
	if (showRightEllipsis) pages.push(null)
	pages.push(total)
	return pages
}

/** "1–20 of 137" summary shown beside the control. */
export function paginationSummary(meta: { page: number; per_page: number; total: number }): string {
	if (meta.total === 0) return "No results"
	const first = (meta.page - 1) * meta.per_page + 1
	const last = Math.min(meta.page * meta.per_page, meta.total)
	return `${first}–${last} of ${meta.total}`
}
