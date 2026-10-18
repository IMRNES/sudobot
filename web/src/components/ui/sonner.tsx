import {
	CircleCheckIcon,
	InfoIcon,
	Loader2Icon,
	OctagonXIcon,
	TriangleAlertIcon,
} from "lucide-react"
import { Toaster as Sonner, type ToasterProps } from "sonner"
import { useSession } from "#/libs/auth/session-context.tsx"
import { cn } from "#/libs/utils.ts"

/**
 * App-wide toast surface. The theme comes from `SessionProvider` rather than
 * `next-themes`, because the theme preference lives in the same store as the
 * session (`libs/auth/session.ts`).
 */
export function Toaster({ className, ...props }: ToasterProps) {
	const { theme } = useSession()

	return (
		<Sonner
			theme={theme}
			className={cn("toaster group", className)}
			position="bottom-right"
			closeButton
			icons={{
				success: <CircleCheckIcon className="size-4" />,
				info: <InfoIcon className="size-4" />,
				warning: <TriangleAlertIcon className="size-4" />,
				error: <OctagonXIcon className="size-4" />,
				loading: <Loader2Icon className="size-4 animate-spin" />,
			}}
			style={
				{
					"--normal-bg": "var(--popover)",
					"--normal-text": "var(--popover-foreground)",
					"--normal-border": "var(--border)",
					"--border-radius": "var(--radius)",
				} as React.CSSProperties
			}
			{...props}
		/>
	)
}
