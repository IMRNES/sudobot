import { useNavigate, useSearch } from "@tanstack/react-router"
import { useCallback, useMemo } from "react"
import { DEFAULT_PER_PAGE, type TListParamsState } from "#/hooks/use-pagination.ts"

/**
 * Table state stored in the route's search params.
 *
 * Route-level lists use this rather than local state so "page 3, filtered by
 * `ada`, sorted by level" survives a reload and can be pasted to a teammate.
 * The schema lives in `_data/table-search.ts` and is attached to each list route
 * through `validateSearch`, which is what lets these values round-trip as
 * numbers instead of strings.
 */

export interface ITableSearchParams {
	page: number
	per_page: number
	search: string
	sort_by: string | undefined
	sort_direction: "asc" | "desc" | undefined
}

export interface IUseTableSearchResult {
	page: number
	perPage: number
	search: string
	sortBy: string | undefined
	sortDirection: "asc" | "desc" | undefined
	/** Params ready to hand straight to a `_data/` query factory. */
	params: TListParamsState
	setPage: (page: number) => void
	setPerPage: (perPage: number) => void
	setSearch: (search: string) => void
	setSort: (sortBy: string, direction?: "asc" | "desc") => void
}

export interface IUseTableSearchOptions {
	defaultPerPage?: number
	defaultSortBy?: string
	defaultSortDirection?: "asc" | "desc"
}

export function useTableSearch<TFrom extends string>(
	from: TFrom,
	options: IUseTableSearchOptions = {},
): IUseTableSearchResult {
	const { defaultPerPage = DEFAULT_PER_PAGE, defaultSortBy, defaultSortDirection } = options

	// `strict: false` because the caller supplies the route id as a plain string;
	// the values are re-validated by the route's own `validateSearch` anyway.
	const search = useSearch({ strict: false }) as Partial<ITableSearchParams>
	const navigate = useNavigate()

	const page = search.page ?? 1
	const perPage = search.per_page ?? defaultPerPage
	const term = search.search ?? ""
	const sortBy = search.sort_by ?? defaultSortBy
	const sortDirection = search.sort_direction ?? defaultSortDirection

	const update = useCallback(
		(patch: Partial<ITableSearchParams>) => {
			void navigate({
				to: from as never,
				search: ((previous: Record<string, unknown>) => ({ ...previous, ...patch })) as never,
				// `replace` keeps paging out of the history stack, so Back leaves the
				// page rather than walking through 40 pages of a table.
				replace: true,
			})
		},
		[navigate, from],
	)

	return {
		page,
		perPage,
		search: term,
		sortBy,
		sortDirection,
		params: useMemo<TListParamsState>(
			() => ({
				page,
				per_page: perPage,
				search: term.trim().length > 0 ? term.trim() : undefined,
				sort_by: sortBy,
				sort_direction: sortDirection,
			}),
			[page, perPage, term, sortBy, sortDirection],
		),
		setPage: (next) => update({ page: Math.max(1, next) }),
		setPerPage: (next) => update({ per_page: next, page: 1 }),
		setSearch: (next) => update({ search: next, page: 1 }),
		setSort: (nextSortBy, direction = "asc") =>
			update({ sort_by: nextSortBy, sort_direction: direction, page: 1 }),
	}
}
