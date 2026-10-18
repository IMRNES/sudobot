import { api } from "#/libs/api/client.ts"
import type {
	IAuthorizeResponse,
	ICallbackRequest,
	ICallbackResponse,
	ILogoutResponse,
	IMeResponse,
	IRefreshResponse,
} from "#/libs/api/types.ts"
import { storeOAuthState } from "#/libs/auth/session.ts"

/**
 * The `/auth/*` surface of the API, plus `/me`.
 *
 * These calls all opt out of the bearer header — they are either the way to
 * obtain a token (`authorize`, `callback`, `refresh`) or explicitly public.
 */
export const authApi = {
	/**
	 * Ask the server for a Discord authorize URL plus the `state` nonce.
	 *
	 * The caller is responsible for persisting `state` (via `storeOAuthState`)
	 * before redirecting; the callback verifies it to close the CSRF hole.
	 */
	authorize: () =>
		api.post<IAuthorizeResponse>("/auth/discord/authorize", {
			auth: false,
		}),

	/** Exchange the `code` Discord handed back for a token pair and the identity. */
	callback: (payload: ICallbackRequest) =>
		api.post<ICallbackResponse>("/auth/discord/callback", {
			auth: false,
			body: payload,
		}),

	/**
	 * Rotate the token pair. `skipRefresh` is essential: this is the call the
	 * client's own 401 handler makes, so letting it recurse would loop.
	 */
	refresh: (refreshToken: string) =>
		api.post<IRefreshResponse>("/auth/refresh", {
			auth: false,
			skipRefresh: true,
			body: { refresh_token: refreshToken },
		}),

	/** Revoke one refresh token, or every token for the user. */
	logout: (refreshToken: string, everywhere = false) =>
		api.post<ILogoutResponse>("/auth/logout", {
			auth: false,
			body: { refresh_token: refreshToken, everywhere },
		}),

	/** Current user plus the guilds they can see. The app's session root. */
	me: (signal?: AbortSignal) => api.get<IMeResponse>("/me", { signal }),

	health: () => api.get<{ status: string; database: string }>("/health", { auth: false }),
}

/* -------------------------------------------------------------------------- */
/* Imperative helpers                                                          */
/* -------------------------------------------------------------------------- */

/**
 * Step one of sign-in: fetch the authorize URL, remember `state`, and hand the
 * browser to Discord. Kept out of React so both the landing page and the
 * "session expired" redirect can call it.
 */
export async function beginDiscordSignIn(): Promise<void> {
	const { authorize_url: authorizeUrl, state } = await authApi.authorize()
	// Persist the nonce *before* leaving the page; `/auth/callback` compares the
	// `state` Discord echoes back against this value.
	storeOAuthState(state)
	window.location.assign(authorizeUrl)
}
