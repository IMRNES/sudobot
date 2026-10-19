import type { IListParams, TListResponse } from "#/libs/api/types.ts"
import { sessionStore } from "#/libs/auth/session.ts"
import { ApiError, apiErrorFromResponse } from "#/libs/errors/api-error.ts"

/**
 * Thin typed wrapper over `fetch`.
 *
 * Responsibilities, in order:
 *  1. resolve the base URL and append the `/api/v1/iam` API prefix;
 *  2. attach `Authorization: Bearer <access token>` unless the route is public;
 *  3. on `401`, refresh **once** and replay the original request — concurrent
 *     401s share a single refresh promise, because the API rotates refresh
 *     tokens and two parallel refreshes would invalidate each other;
 *  4. throw `ApiError` for every non-2xx so callers never inspect `Response`.
 */

/** Every route lives under this prefix (API.md, "Base path"). */
const API_PREFIX = "/api/v1/iam"

/**
 * Origin of the Axum service, from `VITE_API_BASE_URL` (see `web/.env.example`).
 * An empty value is valid and means "same origin", which is what a
 * reverse-proxied deployment wants.
 */
export const API_BASE_URL: string = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/+$/, "")

export function apiUrl(path: string): string {
	const normalised = path.startsWith("/") ? path : `/${path}`
	return `${API_BASE_URL}${API_PREFIX}${normalised}`
}

type TQueryValue = string | number | boolean | null | undefined

export interface IRequestOptions {
	method?: "GET" | "POST" | "PATCH" | "PUT" | "DELETE"
	/** JSON body; serialised for you. */
	body?: unknown
	/** Query values. `undefined`, `null` and `""` entries are dropped; arrays repeat the key. */
	query?: Record<string, TQueryValue | TQueryValue[]>
	/** Set `false` for `/auth/*` routes, which must not carry a bearer header. */
	auth?: boolean
	/** Skip the refresh-and-retry step (used by the refresh call itself). */
	skipRefresh?: boolean
	signal?: AbortSignal
}

/** Serialise query values, expanding arrays into repeated keys. */
export function buildQueryString(query: IRequestOptions["query"]): string {
	if (!query) return ""
	const search = new URLSearchParams()
	for (const [key, value] of Object.entries(query)) {
		const entries = Array.isArray(value) ? value : [value]
		for (const entry of entries) {
			if (entry === undefined || entry === null || entry === "") continue
			search.append(key, String(entry))
		}
	}
	const serialised = search.toString()
	return serialised.length > 0 ? `?${serialised}` : ""
}

/** Turn `IListParams` into the query object the API expects. */
export function listQuery(params: IListParams = {}): Record<string, TQueryValue | TQueryValue[]> {
	return {
		page: params.page,
		per_page: params.per_page,
		sort_by: params.sort_by,
		sort_direction: params.sort_direction,
		search: params.search,
		filter: params.filter,
	}
}

function networkError(error: unknown): ApiError {
	const aborted = error instanceof Error && error.name === "AbortError"
	return new ApiError(
		aborted
			? "The request was cancelled."
			: "Could not reach the API. Check that the server is running and that CORS allows this origin.",
		0,
		aborted ? "aborted" : "network_error",
		null,
	)
}

async function parseBody(response: Response): Promise<unknown> {
	if (response.status === 204) return null
	const text = await response.text()
	if (text.length === 0) return null
	try {
		return JSON.parse(text) as unknown
	} catch {
		return text
	}
}

function errorCodeOf(body: unknown): string | null {
	if (typeof body === "object" && body !== null && "code" in body) {
		const code = (body as { code?: unknown }).code
		return typeof code === "string" ? code : null
	}
	return null
}

/* -------------------------------------------------------------------------- */
/* Refresh                                                                     */
/* -------------------------------------------------------------------------- */

let refreshPromise: Promise<boolean> | null = null

async function performRefresh(): Promise<boolean> {
	const refreshToken = sessionStore.getRefreshToken()
	if (refreshToken === null) return false

	try {
		const response = await fetch(apiUrl("/auth/refresh"), {
			method: "POST",
			headers: { "Content-Type": "application/json", Accept: "application/json" },
			body: JSON.stringify({ refresh_token: refreshToken }),
		})

		if (!response.ok) {
			// The refresh token is invalid or revoked: the session is over.
			sessionStore.clear()
			sessionStore.notifyExpired()
			return false
		}

		sessionStore.setTokens(
			(await response.json()) as {
				access_token: string
				refresh_token: string
				expires_in: number
			},
		)
		return true
	} catch {
		// Network failure, not a rejection: keep the refresh token so a later
		// attempt can still succeed, but report this refresh as failed.
		return false
	}
}

/** Refresh at most once at a time. Resolves to whether a usable token is now held. */
export function refreshSession(): Promise<boolean> {
	if (refreshPromise === null) {
		refreshPromise = performRefresh().finally(() => {
			refreshPromise = null
		})
	}
	return refreshPromise
}

/* -------------------------------------------------------------------------- */
/* Core request                                                                */
/* -------------------------------------------------------------------------- */

async function send(path: string, options: IRequestOptions): Promise<Response> {
	const { method = "GET", body, query, auth = true, signal } = options
	const headers = new Headers({ Accept: "application/json" })

	if (body !== undefined) headers.set("Content-Type", "application/json")

	if (auth) {
		const token = sessionStore.getAccessToken()
		if (token !== null) headers.set("Authorization", `Bearer ${token}`)
	}

	return fetch(`${apiUrl(path)}${buildQueryString(query)}`, {
		method,
		headers,
		body: body === undefined ? undefined : JSON.stringify(body),
		signal,
	})
}

/**
 * Issue a request and return the raw `Response` and its decoded body.
 *
 * Retries exactly once, and only after a successful refresh. A `403` is never
 * retried — a permission problem must not cause a silent second call.
 */
async function execute(
	path: string,
	options: IRequestOptions,
): Promise<{ response: Response; body: unknown }> {
	let response: Response
	try {
		response = await send(path, options)
	} catch (error) {
		throw networkError(error)
	}

	if (response.status === 401 && options.auth !== false && options.skipRefresh !== true) {
		const probe = await parseBody(response.clone())
		const code = errorCodeOf(probe)
		// `token_expired` is the documented case; a bare 401 with no code is
		// treated the same, since a refresh is the only way forward either way.
		if (code === null || code === "token_expired") {
			if (await refreshSession()) {
				return execute(path, { ...options, skipRefresh: true })
			}
		}
	}

	const body = await parseBody(response)
	if (!response.ok) throw apiErrorFromResponse(response.status, body, response.statusText)
	return { response, body }
}

async function request<TResponse>(path: string, options: IRequestOptions = {}): Promise<TResponse> {
	const { body } = await execute(path, options)
	return body as TResponse
}

/** Read the pagination block, preferring the body and falling back to headers. */
export function readListMeta<TItem>(
	body: TListResponse<TItem>,
	response: Response,
): TListResponse<TItem>["meta"] {
	const header = (name: string): number | null => {
		const raw = response.headers.get(name)
		if (raw === null) return null
		const parsed = Number.parseInt(raw, 10)
		return Number.isFinite(parsed) ? parsed : null
	}
	const page = header("X-Current-Page") ?? body.meta?.page ?? 1
	const totalPages = header("X-Total-Pages") ?? body.meta?.total_pages ?? 1
	return {
		page,
		per_page: header("X-Per-Page") ?? body.meta?.per_page ?? 20,
		total: header("X-Total-Count") ?? body.meta?.total ?? body.data?.length ?? 0,
		total_pages: totalPages,
		has_next: body.meta?.has_next ?? page < totalPages,
		has_prev: body.meta?.has_prev ?? page > 1,
	}
}

/**
 * List request. Returns `{ data, meta }` exactly as documented, with `meta`
 * repaired from the `X-*` headers when the body omits it.
 *
 * Accepts **either** shape the API legitimately returns. Paginated routes send
 * `{ data, meta }`; the unpaginated ones (roles, permissions, a guild's rewards,
 * shop items, panels, autoroles, the economy leaderboard) send a bare array,
 * because there is nothing to paginate. Reading only `body.data` made every one of
 * those render as an empty list — data was present and the UI said "none
 * configured", which is the single most misleading failure a list can have.
 *
 * Any `query` passed in `options` is merged over the pagination params, which
 * is how routes with a non-paginated knob (the leaderboards' `?limit=`) stay on
 * the same code path as the paginated ones.
 */
async function requestList<TItem>(
	path: string,
	params: IListParams = {},
	options: Omit<IRequestOptions, "method" | "body"> = {},
): Promise<TListResponse<TItem>> {
	const { query, ...rest } = options
	const { response, body } = await execute(path, {
		...rest,
		query: { ...listQuery(params), ...query },
	})

	// A bare array is a complete answer, not a truncated envelope: synthesise the
	// meta from it so callers get one shape.
	const list: TListResponse<TItem> = Array.isArray(body)
		? {
				data: body as TItem[],
				meta: {
					page: 1,
					per_page: body.length,
					total: body.length,
					total_pages: 1,
					has_next: false,
					has_prev: false,
				},
			}
		: ((body ?? { data: [] }) as TListResponse<TItem>)

	return { data: list.data ?? [], meta: readListMeta(list, response) }
}

/* -------------------------------------------------------------------------- */
/* Facade                                                                      */
/* -------------------------------------------------------------------------- */

export const api = {
	get: <TResponse>(path: string, options?: Omit<IRequestOptions, "method" | "body">) =>
		request<TResponse>(path, { ...options, method: "GET" }),

	post: <TResponse>(path: string, options?: Omit<IRequestOptions, "method">) =>
		request<TResponse>(path, { ...options, method: "POST" }),

	patch: <TResponse>(path: string, options?: Omit<IRequestOptions, "method">) =>
		request<TResponse>(path, { ...options, method: "PATCH" }),

	put: <TResponse>(path: string, options?: Omit<IRequestOptions, "method">) =>
		request<TResponse>(path, { ...options, method: "PUT" }),

	delete: <TResponse>(path: string, options?: Omit<IRequestOptions, "method">) =>
		request<TResponse>(path, { ...options, method: "DELETE" }),

	list: requestList,
}
