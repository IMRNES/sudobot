import { useQueryClient } from "@tanstack/react-query"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useEffect, useRef, useState } from "react"
import { RouteError } from "#/components/layout/route-states.tsx"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { Skeleton } from "#/components/ui/skeleton.tsx"
import { authApi } from "#/libs/api/auth.ts"
import { consumeOAuthState, consumeRedirect, sessionStore } from "#/libs/auth/session.ts"
import { queryKeys } from "#/libs/query/keys.ts"

/**
 * Discord OAuth callback.
 *
 * Sequence, and why each step is where it is:
 *  1. read `code` and `state` from the search params (Discord appends both);
 *  2. compare `state` against the nonce stored before the redirect — a mismatch
 *     means the callback did not originate from this browser, so it is refused
 *     rather than exchanged;
 *  3. exchange the code via `POST /auth/discord/callback`, which returns the
 *     token pair *and* the identity, so the session query is seeded from the
 *     response and the dashboard does not immediately re-fetch `/me`;
 *  4. navigate to `/dashboard`, or back to whatever the user was trying to
 *     reach before the redirect interrupted them.
 *
 * The exchange is guarded by a ref because React 19 StrictMode runs effects
 * twice in development, and a code can only be redeemed once.
 */

interface ICallbackSearch {
	code?: string
	state?: string
	error?: string
	error_description?: string
}

export const Route = createFileRoute("/_public/auth/callback")({
	validateSearch: (search: Record<string, unknown>): ICallbackSearch => ({
		code: typeof search.code === "string" ? search.code : undefined,
		state: typeof search.state === "string" ? search.state : undefined,
		error: typeof search.error === "string" ? search.error : undefined,
		error_description:
			typeof search.error_description === "string" ? search.error_description : undefined,
	}),
	component: AuthCallbackPage,
})

function AuthCallbackPage() {
	const { code, state, error: oauthError, error_description: oauthDescription } = Route.useSearch()
	const navigate = useNavigate()
	const queryClient = useQueryClient()
	const [failure, setFailure] = useState<string | null>(null)
	const exchanged = useRef(false)

	useEffect(() => {
		if (oauthError !== undefined) {
			setFailure(oauthDescription ?? `Discord refused the sign-in (${oauthError}).`)
			return
		}

		if (code === undefined || code === undefined || code.length === 0) {
			setFailure("This link is missing its authorization code. Start the sign-in again.")
			return
		}

		const expectedState = consumeOAuthState()
		if (expectedState === null) {
			setFailure(
				"The sign-in attempt expired or was started in another browser. Please start again.",
			)
			return
		}
		if (state === undefined || state !== expectedState) {
			setFailure("The security check failed. This sign-in attempt was not started by this browser.")
			return
		}

		if (exchanged.current) return
		exchanged.current = true

		void (async () => {
			try {
				const result = await authApi.callback({ code, state })
				sessionStore.setTokens(result)
				// Seed the session query with what the exchange already returned, so
				// the dashboard renders on the first paint instead of showing a
				// skeleton while `/me` runs.
				queryClient.setQueryData(queryKeys.session.me(), {
					user: result.user,
					guilds: result.guilds,
				})
				const destination = consumeRedirect()
				await navigate({ to: destination ?? "/dashboard", replace: true })
			} catch (caught) {
				setFailure(
					caught instanceof Error
						? caught.message
						: "The server could not complete the sign-in. Please try again.",
				)
			}
		})()
	}, [code, state, oauthError, oauthDescription, navigate, queryClient])

	if (failure !== null) {
		return (
			<RouteError
				title="Sign-in failed"
				description="Discord did not hand back a usable authorization, so no session was started."
				message={failure}
				retryLabel="Back to the dashboard"
				onRetry={() => {
					void navigate({ to: "/dashboard", replace: true })
				}}
			/>
		)
	}

	return (
		<div className="flex min-h-svh items-center justify-center px-6">
			<Card className="w-full max-w-md">
				<CardHeader>
					<CardTitle>Finishing sign-in</CardTitle>
					<CardDescription>
						Verifying the response from Discord and starting your session.
					</CardDescription>
				</CardHeader>
				<CardContent className="flex flex-col gap-3">
					<Skeleton className="h-4 w-full" />
					<Skeleton className="h-4 w-3/4" />
				</CardContent>
			</Card>
		</div>
	)
}
