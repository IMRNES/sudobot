import type { TApiErrorBody } from "#/libs/api/types.ts"

/**
 * The single error type every API call rejects with.
 *
 * It carries the HTTP status and the server's `code` so callers can branch —
 * `ApiError.isUnauthorized`, `ApiError.isForbidden` — while `message` is always
 * safe to show in a toast or an inline field error.
 */
export class ApiError extends Error {
	readonly status: number
	readonly code: string | null
	readonly body: unknown

	constructor(message: string, status: number, code: string | null = null, body: unknown = null) {
		super(message)
		this.name = "ApiError"
		this.status = status
		this.code = code
		this.body = body
	}

	get isUnauthorized(): boolean {
		return this.status === 401
	}

	get isForbidden(): boolean {
		return this.status === 403
	}

	get isNotFound(): boolean {
		return this.status === 404
	}

	get isConflict(): boolean {
		return this.status === 409
	}

	get isValidation(): boolean {
		return this.status === 400
	}

	/** The access token expired; the client should refresh and retry once. */
	get isTokenExpired(): boolean {
		return this.status === 401 && this.code === "token_expired"
	}
}

function isErrorBody(value: unknown): value is TApiErrorBody {
	return (
		typeof value === "object" &&
		value !== null &&
		"error" in value &&
		typeof (value as { error: unknown }).error === "string"
	)
}

/** Fallback copy keyed by status, used when the body has no `error` string. */
function fallbackMessage(status: number): string {
	switch (status) {
		case 400:
			return "The server rejected that request."
		case 401:
			return "Your session has expired. Please sign in again."
		case 403:
			return "You do not have permission to do that."
		case 404:
			return "That resource no longer exists."
		case 409:
			return "That conflicts with something that already exists."
		case 500:
			return "The server hit an unexpected error."
		default:
			return `Request failed with status ${status}.`
	}
}

/**
 * Turn anything thrown by a request into an `ApiError`.
 *
 * Network failures and aborted requests arrive as `TypeError` / `DOMException`,
 * so they are folded into the same shape and components only ever see one type.
 */
export function toApiError(error: unknown): ApiError {
	if (error instanceof ApiError) return error

	if (error instanceof DOMException && error.name === "AbortError") {
		return new ApiError("The request was cancelled.", 0, "aborted", null)
	}

	if (error instanceof TypeError) {
		return new ApiError(
			"Could not reach the API. Check that the server is running and CORS allows this origin.",
			0,
			"network_error",
			null,
		)
	}

	if (error instanceof Error) {
		return new ApiError(error.message, 0, "client_error", null)
	}

	return new ApiError("Something went wrong.", 0, "unknown_error", null)
}

/** Build an `ApiError` from a non-2xx response. */
export function apiErrorFromResponse(status: number, body: unknown, statusText = ""): ApiError {
	if (isErrorBody(body)) {
		return new ApiError(body.error, status, body.code ?? null, body)
	}
	const message = statusText.trim().length > 0 ? statusText : fallbackMessage(status)
	return new ApiError(message, status, null, body)
}
