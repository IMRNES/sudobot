import { Search, X } from "lucide-react"
import { useEffect, useState } from "react"
import { Button } from "#/components/ui/button.tsx"
import { Input } from "#/components/ui/input.tsx"
import { cn } from "#/libs/utils.ts"

/**
 * Search box for the list routes.
 *
 * Keystrokes are debounced before they reach the URL, so typing "moderator"
 * issues one request rather than nine. The input keeps its own state while the
 * user types; the committed value is pushed up after the delay.
 */
export function TableSearch({
	value,
	onChange,
	placeholder = "Search…",
	className,
	disabled = false,
	delayMs = 300,
}: {
	value: string
	onChange: (value: string) => void
	placeholder?: string
	className?: string
	disabled?: boolean
	delayMs?: number
}) {
	const [draft, setDraft] = useState(value)

	// Re-sync when the URL value changes from outside (back button, clear).
	useEffect(() => {
		setDraft(value)
	}, [value])

	useEffect(() => {
		if (draft === value) return
		const timer = window.setTimeout(() => onChange(draft), delayMs)
		return () => window.clearTimeout(timer)
	}, [draft, value, onChange, delayMs])

	return (
		<div className={cn("relative w-full sm:max-w-xs", className)}>
			<Search
				className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
				aria-hidden="true"
			/>
			<Input
				type="search"
				value={draft}
				onChange={(event) => setDraft(event.target.value)}
				placeholder={placeholder}
				disabled={disabled}
				aria-label={placeholder}
				className="pl-9"
			/>
			{draft.length > 0 ? (
				<Button
					variant="ghost"
					size="icon-xs"
					onClick={() => {
						setDraft("")
						onChange("")
					}}
					className="absolute top-1/2 right-2 -translate-y-1/2"
					aria-label="Clear search"
				>
					<X />
				</Button>
			) : null}
		</div>
	)
}

/** Row of filters and actions above a table. */
export function TableToolbar({
	children,
	className,
}: {
	children: React.ReactNode
	className?: string
}) {
	return (
		<div className={cn("flex flex-wrap items-center justify-between gap-3", className)}>
			{children}
		</div>
	)
}
