import { cn } from "#/libs/utils.ts"

/**
 * The one empty state used everywhere, so "no data" always looks intentional
 * rather than broken: an optional icon, a headline, one sentence of guidance,
 * and the primary next action.
 */
export function EmptyState({
	title,
	description,
	action,
	icon: Icon,
	className,
}: {
	title: string
	description?: string
	action?: React.ReactNode
	icon?: React.ComponentType<{ className?: string }>
	className?: string
}) {
	return (
		<div
			className={cn(
				"flex flex-col items-center justify-center gap-3 px-6 py-14 text-center",
				className,
			)}
		>
			{Icon ? (
				<span className="grid size-11 place-items-center rounded-full border border-border bg-muted/50">
					<Icon className="size-5 text-muted-foreground" />
				</span>
			) : null}
			<div className="flex flex-col gap-1">
				<p className="text-sm font-medium">{title}</p>
				{description ? (
					<p className="max-w-sm text-sm text-muted-foreground">{description}</p>
				) : null}
			</div>
			{action ? <div className="pt-1">{action}</div> : null}
		</div>
	)
}
