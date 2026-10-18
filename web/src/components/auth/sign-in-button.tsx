import { ArrowRight } from "lucide-react"
import { useState } from "react"
import { toast } from "sonner"
import { Button } from "#/components/ui/button.tsx"
import { beginDiscordSignIn } from "#/libs/api/auth.ts"
import { normalizeApiError } from "#/libs/errors/index.ts"

type TSignInButtonProps = {
	size?: "sm" | "lg"
	label?: string
	variant?: "default" | "secondary" | "outline"
}

/**
 * Starts the Discord OAuth handshake.
 *
 * This is a `<button>`, not a router `<Link to="/dashboard">`, and that is the
 * whole point: `/dashboard` sits behind the `_authenticated` guard, which sends a
 * visitor with no session back to `/`. A link to it therefore bounces off the guard
 * and never reaches `beginDiscordSignIn`, so a signed-out visitor clicking "sign in"
 * lands back on the page they started from with no request made.
 *
 * A 503 (`code: "not_configured"`) means the deployment has no Discord client secret
 * — the login flow is disabled rather than broken, so it is surfaced as a toast
 * instead of leaving the button looking inert.
 */
export function SignInButton({
	size = "lg",
	label = "Sign in with Discord",
	variant,
}: TSignInButtonProps) {
	const [pending, setPending] = useState(false)

	const handleClick = async () => {
		setPending(true)
		try {
			await beginDiscordSignIn()
			// Success navigates away; `pending` is deliberately left set so the
			// button does not flash back to its idle label mid-redirect.
		} catch (error) {
			setPending(false)
			toast.error(normalizeApiError(error).message)
		}
	}

	return (
		<Button size={size} variant={variant} disabled={pending} onClick={() => void handleClick()}>
			{pending ? "Redirecting to Discord…" : label}
			<ArrowRight />
		</Button>
	)
}
