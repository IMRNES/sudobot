import type { IAppError } from "#/libs/api/types.ts"
import { ApiError, toApiError } from "#/libs/errors/api-error.ts"

/**
 * Reduce any thrown value to the flat shape components render.
 *
 * The server's error contract is a single `{"error": "..."}` string, so there
 * are no field keys to split out; `fields` stays empty unless a future route
 * starts returning a per-field payload, which is detected defensively below.
 */
export function normalizeApiError(error: unknown): IAppError {
	const apiError = toApiError(error)

	return {
		message: apiError.message,
		status: apiError.status === 0 ? null : apiError.status,
		code: apiError.code,
		fields: extractFieldErrors(apiError.body),
	}
}

function extractFieldErrors(body: unknown): Record<string, string> {
	if (typeof body !== "object" || body === null) return {}
	const candidate = (body as { errors?: unknown }).errors
	if (typeof candidate !== "object" || candidate === null) return {}

	const fields: Record<string, string> = {}
	for (const [key, value] of Object.entries(candidate)) {
		if (typeof value === "string") fields[key] = value
	}
	return fields
}

/**
 * Message for a form field.
 *
 * The server's error contract is a single string with no per-field keys, so the
 * field-level lookup is the fallback and the summary is what normally shows —
 * but a future payload with `errors: {price: "..."}` is already handled, and
 * callers do not have to change when that lands.
 */
export function fieldError(error: unknown, field: string, fallback?: string): string | undefined {
	if (error === null || error === undefined) return fallback
	const normalised = normalizeApiError(error)
	return normalised.fields[field] ?? fallback ?? normalised.message
}

/** True when the failure is a permissions problem rather than a real fault. */
export function isPermissionError(error: unknown): boolean {
	return toApiError(error).status === 403
}

export { ApiError }
