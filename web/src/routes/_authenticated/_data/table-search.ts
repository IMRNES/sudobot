import { DEFAULT_PER_PAGE } from "#/hooks/use-pagination.ts"
import type { ITableSearchParams } from "#/hooks/use-table-search.ts"

/**
 * `validateSearch` for every list route.
 *
 * Search params arrive as strings (or numbers, when the router serialised them
 * itself), so this coerces and bounds them once, at the edge. The result is a
 * fully-populated `ITableSearchParams`, which means the components downstream
 * never deal with `undefined` page numbers or a `per_page` of `-3`.
 */
export function validateTableSearch(search: Record<string, unknown>): ITableSearchParams {
	return {
		page: positiveInt(search.page, 1),
		per_page: boundedInt(search.per_page, DEFAULT_PER_PAGE, 1, 100),
		search: typeof search.search === "string" ? search.search : "",
		sort_by:
			typeof search.sort_by === "string" && search.sort_by.length > 0 ? search.sort_by : undefined,
		sort_direction:
			search.sort_direction === "desc"
				? "desc"
				: search.sort_direction === "asc"
					? "asc"
					: undefined,
	}
}

/** Subset used by list routes without a sortable column set. */
export function validateBasicTableSearch(
	search: Record<string, unknown>,
): Pick<ITableSearchParams, "page" | "per_page" | "search"> {
	return {
		page: positiveInt(search.page, 1),
		per_page: boundedInt(search.per_page, DEFAULT_PER_PAGE, 1, 100),
		search: typeof search.search === "string" ? search.search : "",
	}
}

function positiveInt(value: unknown, fallback: number): number {
	const parsed = typeof value === "number" ? value : Number.parseInt(String(value ?? ""), 10)
	return Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : fallback
}

function boundedInt(value: unknown, fallback: number, min: number, max: number): number {
	const parsed = positiveInt(value, fallback)
	return Math.min(Math.max(parsed, min), max)
}
