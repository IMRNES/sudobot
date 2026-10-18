import { Slot } from "radix-ui"
import { cn } from "#/libs/utils.ts"

/**
 * Breadcrumb primitives. Generated in the shadcn "breadcrumb" shape, adapted to
 * this project's `cn` alias and explicit file extensions.
 */
function Breadcrumb({ ...props }: React.ComponentProps<"nav">) {
	return <nav aria-label="Breadcrumb" data-slot="breadcrumb" {...props} />
}

function BreadcrumbList({ className, ...props }: React.ComponentProps<"ol">) {
	return (
		<ol
			data-slot="breadcrumb-list"
			className={cn(
				"flex flex-wrap items-center gap-1.5 text-sm break-words text-muted-foreground sm:gap-2.5",
				className,
			)}
			{...props}
		/>
	)
}

function BreadcrumbItem({ className, ...props }: React.ComponentProps<"li">) {
	return (
		<li
			data-slot="breadcrumb-item"
			className={cn("inline-flex items-center gap-1.5", className)}
			{...props}
		/>
	)
}

function BreadcrumbLink({
	asChild,
	className,
	...props
}: React.ComponentProps<"a"> & { asChild?: boolean }) {
	const Comp = asChild ? Slot.Root : "a"
	return (
		<Comp
			data-slot="breadcrumb-link"
			className={cn("rounded-sm transition-colors hover:text-foreground", className)}
			{...props}
		/>
	)
}

function BreadcrumbPage({ className, ...props }: React.ComponentProps<"span">) {
	return (
		<span
			data-slot="breadcrumb-page"
			aria-current="page"
			className={cn("font-normal text-foreground", className)}
			{...props}
		/>
	)
}

function BreadcrumbSeparator({ children, className, ...props }: React.ComponentProps<"li">) {
	return (
		<li
			data-slot="breadcrumb-separator"
			role="presentation"
			aria-hidden="true"
			className={cn("[&>svg]:size-3.5", className)}
			{...props}
		>
			{children ?? "/"}
		</li>
	)
}

export {
	Breadcrumb,
	BreadcrumbItem,
	BreadcrumbLink,
	BreadcrumbList,
	BreadcrumbPage,
	BreadcrumbSeparator,
}
