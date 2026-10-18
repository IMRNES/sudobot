import { Button } from "#/components/ui/button.tsx"
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "#/components/ui/dialog.tsx"

/**
 * Confirmation dialog for destructive actions.
 *
 * The confirm button stays disabled until `confirmLabel` is typed for
 * high-consequence operations (`requireConfirmation`), which is the difference
 * between a mis-click deleting a shop item and a mis-click deleting a role with
 * four members on it.
 */
export function ConfirmDialog({
	open,
	onOpenChange,
	title,
	description,
	confirmLabel = "Confirm",
	cancelLabel = "Cancel",
	onConfirm,
	isPending = false,
	destructive = true,
	/** When set, the user must type this exact string to enable confirmation. */
	requireConfirmation,
	typedValue = "",
	onTypedValueChange,
}: {
	open: boolean
	onOpenChange: (open: boolean) => void
	title: string
	description: React.ReactNode
	confirmLabel?: string
	cancelLabel?: string
	onConfirm: () => void
	isPending?: boolean
	destructive?: boolean
	requireConfirmation?: string
	typedValue?: string
	onTypedValueChange?: (value: string) => void
}) {
	const locked = requireConfirmation !== undefined && typedValue !== requireConfirmation

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>{title}</DialogTitle>
					<DialogDescription asChild>
						<div className="text-sm text-muted-foreground">{description}</div>
					</DialogDescription>
				</DialogHeader>

				{requireConfirmation !== undefined ? (
					<div className="flex flex-col gap-2">
						<label className="text-sm text-muted-foreground" htmlFor="confirm-dialog-input">
							Type <span className="font-mono text-foreground">{requireConfirmation}</span> to
							confirm.
						</label>
						<input
							id="confirm-dialog-input"
							value={typedValue}
							onChange={(event) => onTypedValueChange?.(event.target.value)}
							className="h-9 rounded-md border border-input bg-transparent px-3 text-sm outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
							autoComplete="off"
						/>
					</div>
				) : null}

				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)} disabled={isPending}>
						{cancelLabel}
					</Button>
					<Button
						variant={destructive ? "destructive" : "default"}
						onClick={onConfirm}
						disabled={isPending || locked}
					>
						{isPending ? "Working…" : confirmLabel}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	)
}
