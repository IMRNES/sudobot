import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { Skeleton } from "#/components/ui/skeleton.tsx"
import { cn } from "#/libs/utils.ts"

/**
 * One number with a label, used in the dashboard and guild overview grids.
 * `hint` carries the secondary figure (a delta, a total) so the primary value
 * keeps the visual weight.
 */
export function StatCard({
	label,
	value,
	hint,
	icon: Icon,
	isLoading = false,
	className,
}: {
	label: string
	value: React.ReactNode
	hint?: React.ReactNode
	icon?: React.ComponentType<{ className?: string }>
	isLoading?: boolean
	className?: string
}) {
	return (
		<Card className={cn("gap-0 py-0", className)}>
			<CardHeader className="flex-row items-center justify-between gap-2 px-5 pt-5 pb-2">
				<CardDescription className="text-xs font-medium tracking-wide uppercase">
					{label}
				</CardDescription>
				{Icon ? <Icon className="size-4 text-muted-foreground" /> : null}
			</CardHeader>
			<CardContent className="px-5 pb-5">
				{isLoading ? (
					<Skeleton className="h-8 w-24" />
				) : (
					<CardTitle className="text-2xl tabular-nums">{value}</CardTitle>
				)}
				{hint !== undefined ? <p className="mt-1 text-xs text-muted-foreground">{hint}</p> : null}
			</CardContent>
		</Card>
	)
}
