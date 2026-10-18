import { cn } from "#/libs/utils.ts"

/**
 * The heading block every page starts with: title, one-line description and an
 * optional action cluster on the right. Centralised so vertical rhythm and
 * heading level stay identical across routes.
 */
export function PageHeader({
	title,
	description,
	actions,
	className,
	badge,
}: {
	title: string
	description?: string
	actions?: React.ReactNode
	className?: string
	badge?: React.ReactNode
}) {
	return (
		<header className={cn("flex flex-wrap items-start justify-between gap-4", className)}>
			<div className="flex min-w-0 flex-col gap-1">
				<div className="flex items-center gap-2">
					<h1 className="truncate text-2xl font-semibold tracking-tight">{title}</h1>
					{badge}
				</div>
				{description ? (
					<p className="max-w-2xl text-sm text-muted-foreground">{description}</p>
				) : null}
			</div>
			{actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
		</header>
	)
}
