import { RouterProvider } from "@tanstack/react-router"
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { sessionStore } from "#/libs/auth/session.ts"
import { router } from "#/router.tsx"
import "./styles.css"

/**
 * Entry point.
 *
 * The bootstrap order matters: the expired-session handler is registered before
 * the router mounts, so a refresh that fails during the first render still has
 * somewhere to send the user.
 */
const container = document.getElementById("root")
if (container === null) throw new Error("Missing #root element in index.html.")

// A refresh failure anywhere in the app ends the session and returns the user
// to the landing page, which is the only place that can start a new one.
sessionStore.setExpiredHandler(() => {
	sessionStore.clear()
	void router.navigate({ to: "/", replace: true })
})

createRoot(container).render(
	<StrictMode>
		<RouterProvider router={router} />
	</StrictMode>,
)
