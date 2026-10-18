import { Label } from "#/components/ui/label.tsx"
import { cn } from "#/libs/utils.ts"

/**
 * Label + control + hint/error, in the order a screen reader announces them.
 *
 * Field errors come from two places: the client (required, range) and the
 * server (`{"error": "..."}` on a 400). Both land in `error`, so the field shows
 * one message and the user never has to reconcile two.
 */
export function FormField({
	label,
	htmlFor,
	error,
	description,
	required,
	children,
	className,
	actions,
}: {
	label: string
	htmlFor?: string
	error?: string
	description?: React.ReactNode
	required?: boolean
	children: React.ReactNode
	className?: string
	/** Right-aligned slot on the label row, e.g. a "reset" link. */
	actions?: React.ReactNode
}) {
	return (
		<div className={cn("flex flex-col gap-2", className)}>
			<div className="flex items-center justify-between gap-2">
				<Label htmlFor={htmlFor}>
					{label}
					{required ? <span className="text-destructive">*</span> : null}
				</Label>
				{actions}
			</div>
			{children}
			{error !== undefined && error.length > 0 ? (
				<p className="text-xs text-destructive" role="alert">
					{error}
				</p>
			) : description !== undefined ? (
				<p className="text-xs text-muted-foreground">{description}</p>
			) : null}
		</div>
	)
}

/** Two-or-more fields side by side on wide screens, stacked on narrow ones. */
export function FormRow({
	children,
	className,
	columns = 2,
}: {
	children: React.ReactNode
	className?: string
	columns?: 2 | 3
}) {
	return (
		<div
			className={cn(
				"grid gap-4",
				columns === 2 ? "sm:grid-cols-2" : "sm:grid-cols-2 lg:grid-cols-3",
				className,
			)}
		>
			{children}
		</div>
	)
}
