import type { IAuthTokens } from "#/libs/api/types.ts"

/**
 * Token storage.
 *
 * SECURITY NOTE — this is a development-grade choice, not a production one.
 * The access token lives **in memory only** (a page reload loses it and the
 * client silently refreshes), while the refresh token is kept in
 * `localStorage` because there is no cookie-backed session available: the Axum
 * service is a separate origin from the SPA and issues bearer tokens directly.
 * A production deployment should instead have `/auth/discord/callback` set the
 * refresh token as an `HttpOnly; Secure; SameSite=Lax` cookie and keep only the
 * short-lived access token in memory, which removes the XSS read path that
 * `localStorage` opens. Nothing outside `libs/api/client.ts` and this module
 * should ever touch the raw tokens, so swapping the transport later is a
 * one-file change.
 */

const REFRESH_TOKEN_KEY = "sudobot.refresh_token"
const THEME_KEY = "sudobot.theme"

/** Treat a token as expired slightly early to avoid a race with the server. */
const EXPIRY_SKEW_MS = 15_000

type TListener = () => void

let accessToken: string | null = null
let expiresAt: number | null = null
const listeners = new Set<TListener>()
let expiredHandler: (() => void) | null = null

function readRefreshToken(): string | null {
	try {
		return window.localStorage.getItem(REFRESH_TOKEN_KEY)
	} catch {
		// Private mode / blocked storage: fall back to memory-only for this tab.
		return null
	}
}

function writeRefreshToken(token: string | null): void {
	try {
		if (token === null) window.localStorage.removeItem(REFRESH_TOKEN_KEY)
		else window.localStorage.setItem(REFRESH_TOKEN_KEY, token)
	} catch {
		// Ignore: the in-memory access token still works for this page's lifetime.
	}
}

function notify(): void {
	for (const listener of listeners) listener()
}

export const sessionStore = {
	subscribe(listener: TListener): () => void {
		listeners.add(listener)
		return () => {
			listeners.delete(listener)
		}
	},

	getAccessToken(): string | null {
		return accessToken
	},

	getRefreshToken(): string | null {
		return readRefreshToken()
	},

	/** True while the in-memory access token is present and unexpired. */
	hasValidAccessToken(): boolean {
		if (accessToken === null) return false
		if (expiresAt === null) return true
		return Date.now() < expiresAt - EXPIRY_SKEW_MS
	},

	/** True when a session can be resumed — a refresh token is on disk. */
	hasSession(): boolean {
		return accessToken !== null || readRefreshToken() !== null
	},

	/** Store a freshly issued pair. Access token in memory, refresh on disk. */
	setTokens(tokens: IAuthTokens): void {
		accessToken = tokens.access_token
		expiresAt = Date.now() + tokens.expires_in * 1000
		writeRefreshToken(tokens.refresh_token)
		notify()
	},

	/** Forget everything. Called on logout and on a failed refresh. */
	clear(): void {
		const hadSession = sessionStore.hasSession()
		accessToken = null
		expiresAt = null
		writeRefreshToken(null)
		if (hadSession) notify()
	},

	/**
	 * Register the single handler invoked when a refresh fails while the user is
	 * mid-session, so the app can bounce them to sign-in. Wired in `main.tsx`.
	 */
	setExpiredHandler(handler: (() => void) | null): void {
		expiredHandler = handler
	},

	/** Fire the expired handler if one is registered. */
	notifyExpired(): void {
		expiredHandler?.()
	},
}

/**
 * `sessionStorage` key for the OAuth `state` value. It is deliberately session
 * scoped: the value only has to survive the round trip to Discord and back, and
 * keeping it out of `localStorage` means a stale state cannot be replayed later.
 */
export const OAUTH_STATE_KEY = "sudobot.oauth_state"

export function storeOAuthState(state: string): void {
	try {
		window.sessionStorage.setItem(OAUTH_STATE_KEY, state)
	} catch {
		// Non-fatal: the callback then fails the state check and re-authorises.
	}
}

/** Read and consume the stored OAuth state (one shot). */
export function consumeOAuthState(): string | null {
	try {
		const state = window.sessionStorage.getItem(OAUTH_STATE_KEY)
		window.sessionStorage.removeItem(OAUTH_STATE_KEY)
		return state
	} catch {
		return null
	}
}

/** Where the signed-in destination is remembered across a full-page redirect. */
export const REDIRECT_KEY = "sudobot.redirect"

export function storeRedirect(path: string): void {
	try {
		window.sessionStorage.setItem(REDIRECT_KEY, path)
	} catch {
		/* ignore */
	}
}

export function consumeRedirect(): string | null {
	try {
		const path = window.sessionStorage.getItem(REDIRECT_KEY)
		window.sessionStorage.removeItem(REDIRECT_KEY)
		return path
	} catch {
		return null
	}
}

/* -------------------------------------------------------------------------- */
/* Theme preference (dark-first)                                              */
/* -------------------------------------------------------------------------- */

export type TTheme = "dark" | "light"

export function readStoredTheme(): TTheme {
	try {
		return window.localStorage.getItem(THEME_KEY) === "light" ? "light" : "dark"
	} catch {
		return "dark"
	}
}

export function applyTheme(theme: TTheme): void {
	const root = document.documentElement
	root.classList.toggle("dark", theme === "dark")
	root.classList.toggle("light", theme === "light")
	try {
		window.localStorage.setItem(THEME_KEY, theme)
	} catch {
		/* ignore */
	}
}
