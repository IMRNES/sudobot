import type { VariantProps } from "class-variance-authority"
import { Button, type buttonVariants } from "#/components/ui/button.tsx"
import { Tooltip, TooltipContent, TooltipTrigger } from "#/components/ui/tooltip.tsx"
import { cn } from "#/libs/utils.ts"

/**
 * A button that a permission can turn off.
 *
 * The rule across this dashboard: a control the member cannot use is
 * **disabled with an explanation**, never removed. Hiding it would leave a
 * read-only member unable to tell whether a feature exists, is misconfigured,
 * or simply is not theirs — three different problems with three different
 * fixes.
 *
 * The tooltip is attached to a wrapping `span` because a disabled `<button>`
 * emits no pointer events and would never trigger it.
 */
export function PermissionButton({
	allowed,
	reason,
	disabled,
	children,
	className,
	...props
}: React.ComponentProps<"button"> &
	VariantProps<typeof buttonVariants> & {
		/** Whether the member holds the permission this control needs. */
		allowed: boolean
		/** Tooltip copy shown when `allowed` is false. */
		reason: string
	}) {
	const blocked = !allowed
	const button = (
		<Button
			{...props}
			disabled={disabled || blocked}
			// Screen-reader users get the same explanation as sighted ones.
			aria-disabled={blocked || disabled || undefined}
			title={blocked ? reason : undefined}
			className={cn(blocked && "cursor-not-allowed", className)}
		>
			{children}
		</Button>
	)

	if (!blocked) return button

	return (
		<Tooltip>
			<TooltipTrigger asChild>
				{/* biome-ignore lint/a11y/noNoninteractiveTabindex: the wrapper only exists to receive hover for the tooltip */}
				<span tabIndex={0} className="inline-flex cursor-not-allowed">
					{button}
				</span>
			</TooltipTrigger>
			<TooltipContent>{reason}</TooltipContent>
		</Tooltip>
	)
}

/**
 * Wraps any control (not just a button) so it can be disabled by permission and
 * explain itself on hover. Used for switches, selects and checkboxes.
 */
export function PermissionGate({
	allowed,
	reason,
	children,
	className,
}: {
	allowed: boolean
	reason: string
	children: React.ReactNode
	className?: string
}) {
	if (allowed) return <>{children}</>

	return (
		<Tooltip>
			<TooltipTrigger asChild>
				{/* biome-ignore lint/a11y/noNoninteractiveTabindex: the wrapper only exists to receive hover for the tooltip */}
				<span tabIndex={0} className={cn("inline-flex cursor-not-allowed", className)}>
					<span className="pointer-events-none opacity-50">{children}</span>
				</span>
			</TooltipTrigger>
			<TooltipContent>{reason}</TooltipContent>
		</Tooltip>
	)
}
