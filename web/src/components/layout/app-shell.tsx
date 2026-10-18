import { Link, Outlet, useRouterState } from "@tanstack/react-router"
import { Menu, Moon, Sun } from "lucide-react"
import { useState } from "react"
import { Sidebar } from "#/components/layout/sidebar.tsx"
import { Button } from "#/components/ui/button.tsx"
import { Sheet, SheetContent, SheetTitle, SheetTrigger } from "#/components/ui/sheet.tsx"
import { useSession } from "#/libs/auth/session-context.tsx"
import { cn } from "#/libs/utils.ts"

/**
 * The chrome every authenticated page sits inside.
 *
 * The sidebar is fixed on desktop and slides in from a `Sheet` on narrow
 * screens, so the same navigation works on a phone without a second component.
 */
export function AppShell({
	guildSlug,
	children,
	className,
}: {
	/** Current guild id from the URL, when the route is guild-scoped. */
	guildSlug?: string
	children?: React.ReactNode
	className?: string
}) {
	const [mobileOpen, setMobileOpen] = useState(false)
	const { theme, toggleTheme } = useSession()
	const pathname = useRouterState({ select: (state) => state.location.pathname })

	return (
		<div className={cn("flex min-h-svh w-full bg-background", className)}>
			<a href="#main-content" className="skip-to-content">
				Skip to content
			</a>

			{/* Desktop rail */}
			<aside className="hidden w-64 shrink-0 border-r border-sidebar-border bg-sidebar lg:flex lg:flex-col">
				<Sidebar guildSlug={guildSlug} />
			</aside>

			{/* Mobile drawer */}
			<Sheet open={mobileOpen} onOpenChange={setMobileOpen}>
				<div className="flex min-w-0 flex-1 flex-col">
					<header className="sticky top-0 z-30 flex h-14 items-center gap-3 border-b border-border bg-background/85 px-4 backdrop-blur lg:px-8">
						<SheetTrigger asChild>
							<Button
								variant="ghost"
								size="icon"
								className="lg:hidden"
								aria-label="Open navigation"
							>
								<Menu />
							</Button>
						</SheetTrigger>

						<Link
							to="/dashboard"
							className="text-sm font-semibold tracking-tight text-foreground lg:hidden"
						>
							sudobot
						</Link>

						<div className="flex-1" />

						<Button
							variant="ghost"
							size="icon"
							onClick={toggleTheme}
							aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
						>
							{theme === "dark" ? <Sun /> : <Moon />}
						</Button>
					</header>

					<main
						id="main-content"
						className="min-w-0 flex-1 px-4 py-6 lg:px-8 lg:py-8"
						data-pathname={pathname}
					>
						{children ?? <Outlet />}
					</main>
				</div>

				<SheetContent side="left" className="w-72 bg-sidebar p-0">
					<SheetTitle className="sr-only">Navigation</SheetTitle>
					<Sidebar guildSlug={guildSlug} onNavigate={() => setMobileOpen(false)} />
				</SheetContent>
			</Sheet>
		</div>
	)
}
