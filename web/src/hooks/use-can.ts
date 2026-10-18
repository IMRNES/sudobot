import { useCallback, useMemo } from "react"
import type { IGuildSummary } from "#/libs/api/types.ts"

/**
 * Guild-scoped permission checks.
 *
 * Permissions arrive on each `GuildSummary` as dotted keys (`economy:write`).
 * The convention across the app is that a permission implies nothing about its
 * siblings: `economy:write` does **not** grant `economy:read`, so write controls
 * check the write key and list views check the read key, independently.
 *
 * Unknown permissions are simply not granted, and an unresolvable guild denies
 * rather than silently allowing.
 */

export interface ICanResult {
	/** Whether the action is permitted. */
	allowed: boolean
	/** Ready-to-render explanation, for a tooltip on a disabled control. */
	reason: string
	/** The permission key(s) that were checked, handy for diagnostics. */
	permission: string
}

/** Human copy for a denied action, e.g. `Requires the "economy:write" permission.` */
export function permissionDeniedReason(permission: string): string {
	return `Requires the "${permission}" permission. Ask a server admin to grant it to your role.`
}

/**
 * Check one permission against a guild.
 *
 * `useCan("economy:write")` when a guild is in context, or
 * `useCan("economy:write", guild)` when it is not (e.g. a list row rendering
 * outside the guild layout).
 */
export function useCan(permission: string, guild?: IGuildSummary | null): ICanResult {
	// `guild.permissions` is a stable array reference for as long as the session
	// payload is unchanged, so depending on it directly is both correct and cheap.
	const permissions = guild?.permissions

	return useMemo(() => {
		const allowed = permissions?.includes(permission) ?? false
		return {
			allowed,
			reason: allowed ? "" : permissionDeniedReason(permission),
			permission,
		}
	}, [permissions, permission])
}

/**
 * Check several permissions at once. `mode: "all"` (default) requires every
 * key; `mode: "any"` requires at least one.
 *
 * The caller passes a slightly wider type than `string[]` so a literal tuple can
 * be handed over without a cast; the joined form is what the memo compares on,
 * which keeps an inline array literal from invalidating it on every render.
 */
export function useCanAll(
	permissions: readonly string[],
	guild?: IGuildSummary | null,
	mode: "all" | "any" = "all",
): ICanResult {
	const granted = guild?.permissions
	const key = permissions.join("\u0000")

	return useMemo(() => {
		const wanted = key.length === 0 ? [] : key.split("\u0000")
		const held = granted ?? []
		const missing = wanted.filter((permission) => !held.includes(permission))
		const allowed =
			wanted.length === 0
				? true
				: mode === "all"
					? missing.length === 0
					: missing.length < wanted.length
		return {
			allowed,
			reason: allowed ? "" : missing.map(permissionDeniedReason).join(" "),
			permission: wanted.join(", "),
		}
	}, [granted, key, mode])
}

/** A stable predicate, for `disabled={!can("economy:write")}` style call sites. */
export function useCanFn(guild?: IGuildSummary | null): (permission: string) => boolean {
	const permissions = guild?.permissions
	return useCallback(
		(permission: string) => (permissions ?? []).includes(permission),
		[permissions],
	)
}
