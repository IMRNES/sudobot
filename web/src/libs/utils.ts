import { type ClassValue, clsx } from "clsx"
import { twMerge } from "tailwind-merge"

/** Conditional class names with Tailwind conflict resolution. */
export function cn(...inputs: ClassValue[]): string {
	return twMerge(clsx(inputs))
}

/** `1234567` -> `1,234,567`. Used for balances, XP and counts. */
export function formatNumber(value: number): string {
	return new Intl.NumberFormat(undefined).format(value)
}

/** Signed formatting for ledger amounts: `+500` / `-250`. */
export function formatSigned(value: number): string {
	const formatted = formatNumber(Math.abs(value))
	return value < 0 ? `-${formatted}` : `+${formatted}`
}

/** Compact relative time ("3 minutes ago") with an absolute fallback. */
export function formatRelativeTime(iso: string | null | undefined): string {
	if (!iso) return "never"
	const date = new Date(iso)
	if (Number.isNaN(date.getTime())) return "unknown"
	const deltaSeconds = Math.round((date.getTime() - Date.now()) / 1000)
	const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" })
	const units: [Intl.RelativeTimeFormatUnit, number][] = [
		["year", 31_536_000],
		["month", 2_592_000],
		["week", 604_800],
		["day", 86_400],
		["hour", 3_600],
		["minute", 60],
		["second", 1],
	]
	for (const [unit, seconds] of units) {
		if (Math.abs(deltaSeconds) >= seconds || unit === "second") {
			return formatter.format(Math.round(deltaSeconds / seconds), unit)
		}
	}
	return date.toLocaleString()
}

/** Absolute timestamp for tooltips and tables. */
export function formatDateTime(iso: string | null | undefined): string {
	if (!iso) return "—"
	const date = new Date(iso)
	if (Number.isNaN(date.getTime())) return "—"
	return date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })
}

/**
 * `#RRGGBB` for a Discord embed colour integer, or `null` when unset.
 * The API stores colours as 24-bit ints (`5793266` -> `#5865F2`).
 */
export function numberToHexColor(value: number | null | undefined): string | null {
	if (value === null || value === undefined) return null
	if (!Number.isFinite(value) || value < 0 || value > 0xffffff) return null
	return `#${value.toString(16).padStart(6, "0")}`
}

/** Inverse of `numberToHexColor`; returns 0 for an unparseable string. */
export function hexColorToNumber(hex: string): number {
	const normalised = hex.replace(/^#/, "").trim()
	if (!/^[0-9a-fA-F]{6}$/.test(normalised)) return 0
	return Number.parseInt(normalised, 16)
}

/**
 * Avatar URL with a deterministic fallback.
 *
 * Discord's CDN returns 404s for deleted assets, so the fallback initials are
 * rendered by the `Avatar` primitive instead of pointing at a second URL.
 */
export function avatarUrl(url: string | null | undefined, size = 64): string | null {
	if (!url) return null
	if (!url.startsWith("http")) return null
	const separator = url.includes("?") ? "&" : "?"
	return `${url}${separator}size=${size}`
}

/** First letters of a display name, for avatar fallbacks. */
export function initialsOf(name: string): string {
	const parts = name.trim().split(/\s+/).slice(0, 2)
	return parts.map((part) => part.charAt(0).toUpperCase()).join("") || "?"
}

/** Truncate a snowflake or id for display in dense table cells. */
export function shortId(id: string, visible = 6): string {
	if (id.length <= visible * 2) return id
	return `${id.slice(0, visible)}…${id.slice(-visible)}`
}

/** Human duration from seconds, e.g. `600` -> `10 minutes`. */
export function formatDuration(seconds: number): string {
	if (seconds <= 0) return "immediately"
	const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: "auto", style: "long" })
	const units: [Intl.RelativeTimeFormatUnit, number][] = [
		["day", 86_400],
		["hour", 3_600],
		["minute", 60],
		["second", 1],
	]
	for (const [unit, size] of units) {
		if (seconds >= size || unit === "second") {
			return formatter.format(Math.round(seconds / size), unit).replace(/^in /, "")
		}
	}
	return `${seconds}s`
}

/** 0..1 -> `62%`. */
export function formatPercent(ratio: number, fractionDigits = 0): string {
	return `${(ratio * 100).toFixed(fractionDigits)}%`
}
