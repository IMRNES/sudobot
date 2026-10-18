import { PageHeader } from "#/components/layout/page-header.tsx"
import { Alert, AlertDescription, AlertTitle } from "#/components/ui/alert.tsx"
import { Button } from "#/components/ui/button.tsx"
import { Skeleton } from "#/components/ui/skeleton.tsx"

/**
 * Shown by a guard while it decides whether the visitor may proceed, and by
 * pages whose own data is still loading. A skeleton rather than a spinner keeps
 * the layout from jumping when the real content arrives.
 *
 * Stable keys for the placeholder cards.
 *
 * Literal strings rather than `Array.from({length}, (_, i) => i)`: an index key
 * tells React nothing, and the lint that flags it is right even when the list
 * never reorders.
 */
const SKELETON_KEYS = ["stat-1", "stat-2", "stat-3", "stat-4"] as const

export function RoutePending({ label = "Loading" }: { label?: string }) {
	return (
		<div className="flex flex-col gap-6" aria-busy="true" aria-live="polite">
			<span className="sr-only">{label}</span>
			<Skeleton className="h-8 w-56" />
			<Skeleton className="h-4 w-80" />
			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
				{/* A fixed set of placeholders, so the key is a stable identity rather
				    than an array position that React would have to re-match. */}
				{SKELETON_KEYS.map((key) => (
					<Skeleton key={key} className="h-24 rounded-xl" />
				))}
			</div>
			<Skeleton className="h-64 rounded-xl" />
		</div>
	)
}

/**
 * Full-page failure state for a route whose loader threw. The API's own message
 * is surfaced verbatim — it is written for humans — with a retry that re-runs
 * the route's loaders.
 */
export function RouteError({
	title = "This page could not be loaded",
	message,
	description = "The request to the API did not succeed.",
	onRetry,
	retryLabel = "Try again",
}: {
	title?: string
	message: string
	/** One line of context above the server's own message. */
	description?: string
	onRetry?: () => void
	retryLabel?: string
}) {
	return (
		<div className="mx-auto flex max-w-xl flex-col gap-6 py-12">
			<PageHeader title={title} description={description} />
			<Alert variant="destructive">
				<AlertTitle>Request failed</AlertTitle>
				<AlertDescription>{message}</AlertDescription>
			</Alert>
			{onRetry ? (
				<Button className="self-start" onClick={onRetry}>
					{retryLabel}
				</Button>
			) : null}
		</div>
	)
}

/** Route-level "you cannot see this" state, distinct from a 404. */
export function RouteForbidden({ permission }: { permission?: string }) {
	return (
		<div className="mx-auto flex max-w-xl flex-col gap-6 py-12">
			<PageHeader
				title="You do not have access to this page"
				description="Your role in this server does not include the permission this page needs."
			/>
			<Alert>
				<AlertTitle>Missing permission</AlertTitle>
				<AlertDescription>
					{permission
						? `This page requires "${permission}". Ask a server admin to grant it to your role.`
						: "Ask a server admin to grant your role the permission this page needs."}
				</AlertDescription>
			</Alert>
		</div>
	)
}
