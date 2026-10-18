import {
	createContext,
	type ReactNode,
	useCallback,
	useContext,
	useEffect,
	useMemo,
	useState,
} from "react"
import { api } from "#/libs/api/client.ts"
import type { IAuthTokens, IUser } from "#/libs/api/types.ts"
import { applyTheme, readStoredTheme, sessionStore, type TTheme } from "#/libs/auth/session.ts"

export interface ISessionContextValue {
	/** True when a refresh token is on disk, i.e. the session can be resumed. */
	hasSession: boolean
	/** Convenience passthrough so callers do not import the store directly. */
	clearSession: () => void
	/** Revoke the refresh token server-side, then drop the local session. */
	signOut: (options?: { everywhere?: boolean }) => Promise<void>
	theme: TTheme
	toggleTheme: () => void
}

const SessionContext = createContext<ISessionContextValue | null>(null)

/**
 * Bridges the framework-agnostic `sessionStore` into React so a failed refresh
 * (which clears the store from inside the fetch client) re-renders the tree.
 */
export function SessionProvider({ children }: { children: ReactNode }) {
	const [hasSession, setHasSession] = useState(() => sessionStore.hasSession())
	const [theme, setTheme] = useState<TTheme>(() => readStoredTheme())

	useEffect(() => sessionStore.subscribe(() => setHasSession(sessionStore.hasSession())), [])

	useEffect(() => {
		applyTheme(theme)
	}, [theme])

	const clearSession = useCallback(() => {
		sessionStore.clear()
	}, [])

	const signOut = useCallback(async (options?: { everywhere?: boolean }) => {
		const refreshToken = sessionStore.getRefreshToken()
		if (refreshToken !== null) {
			try {
				// Best effort: if the server is unreachable we still drop the session
				// locally rather than trapping the user in a signed-in shell.
				await api.post("/auth/logout", {
					body: { refresh_token: refreshToken, everywhere: options?.everywhere ?? false },
					auth: false,
				})
			} catch {
				/* ignore */
			}
		}
		sessionStore.clear()
	}, [])

	const value = useMemo<ISessionContextValue>(
		() => ({
			hasSession,
			clearSession,
			signOut,
			theme,
			toggleTheme: () => setTheme((current) => (current === "dark" ? "light" : "dark")),
		}),
		[hasSession, clearSession, signOut, theme],
	)

	return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>
}

export function useSession(): ISessionContextValue {
	const context = useContext(SessionContext)
	if (context === null) {
		throw new Error("useSession must be used inside <SessionProvider>.")
	}
	return context
}

/** Store an issued token pair. Exposed so the callback route stays declarative. */
export function saveTokens(tokens: IAuthTokens): void {
	sessionStore.setTokens(tokens)
}

/** Convenience for shells that only need the identity, not the guilds. */
export type TSessionUser = IUser
