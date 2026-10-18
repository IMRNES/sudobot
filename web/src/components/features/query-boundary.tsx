import { Alert, AlertDescription, AlertTitle } from "#/components/ui/alert.tsx"
import { Skeleton } from "#/components/ui/skeleton.tsx"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { cn } from "#/libs/utils.ts"

/**
 * Renders whatever a query is currently doing, around a single loaded branch.
 *
 * Every data-backed panel in the app goes through this, so "loading", "failed"
 * and "loaded but empty" look the same everywhere and no page silently shows a
 * zero value while its request is still in flight.
 */
export function QueryBoundary({
	isLoading,
	isError,
	error,
	children,
	skeleton,
	empty,
	isEmpty = false,
	className,
}: {
	isLoading: boolean
	isError: boolean
	error: unknown
	children: React.ReactNode
	/** Custom loading placeholder; defaults to a card-shaped skeleton. */
	skeleton?: React.ReactNode
	/** Rendered when `isEmpty` is true and the query succeeded. */
	empty?: React.ReactNode
	isEmpty?: boolean
	className?: string
}) {
	if (isLoading) {
		return <div className={className}>{skeleton ?? <DefaultSkeleton />}</div>
	}

	if (isError) {
		const normalised = normalizeApiError(error)
		return (
			<Alert variant="destructive" className={className}>
				<AlertTitle>Could not load this data</AlertTitle>
				<AlertDescription>
					{normalised.status === 403
						? `Your role does not have permission to read this. (${normalised.message})`
						: normalised.message}
				</AlertDescription>
			</Alert>
		)
	}

	if (isEmpty && empty !== undefined) {
		return <div className={className}>{empty}</div>
	}

	return <div className={className}>{children}</div>
}

function DefaultSkeleton() {
	return (
		<div className="flex flex-col gap-3" aria-busy="true">
			<Skeleton className="h-5 w-40" />
			<Skeleton className="h-24 rounded-xl" />
		</div>
	)
}

/** Inline error line for forms, matching the server's `{"error": "..."}` copy. */
export function FormError({ error, className }: { error: unknown; className?: string }) {
	if (error === null || error === undefined) return null
	const normalised = normalizeApiError(error)
	return (
		<p role="alert" className={cn("text-sm text-destructive", className)}>
			{normalised.message}
		</p>
	)
}
