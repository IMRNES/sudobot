import { Check, ChevronDown, Hash, Search, ShieldAlert } from "lucide-react"
import { useMemo, useState } from "react"
import { Button } from "#/components/ui/button.tsx"
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "#/components/ui/dropdown-menu.tsx"
import { Input } from "#/components/ui/input.tsx"
import type { IDirectoryChannel, IDirectoryRole } from "#/libs/api/types.ts"
import { cn, shortId } from "#/libs/utils.ts"

/**
 * Pickers for the two Discord ids this dashboard stores.
 *
 * A dashboard that asks an admin to paste `1460146703637287115` has moved the
 * work of reading Discord into their lap. These resolve against the guild
 * directory instead, and — because a snowflake can legitimately point at
 * something deleted or invisible to the bot — they always keep a way to type an
 * id by hand. That fallback is not a nicety: it is the only way to configure a
 * role the bot cannot currently see.
 *
 * The two pickers share `Picker`, which owns the only interesting logic: filter
 * by name, resolve the current value, and offer the unresolved id as an explicit
 * entry rather than dropping it.
 */

/** An entry as either picker sees it: an id and something to search. */
interface INamed {
	id: string
	name: string
}

/** True when the text is a plausible snowflake, so a search can be told from an id. */
export function looksLikeSnowflake(value: string): boolean {
	return /^\d{17,20}$/.test(value.trim())
}

function searchByName<T extends INamed>(entries: T[], query: string): T[] {
	const needle = query.trim().toLowerCase()
	if (needle.length === 0) return entries
	return entries.filter((entry) => entry.name.toLowerCase().includes(needle))
}

/**
 * A searchable list of Discord entities.
 *
 * Generic over the entry so both pickers keep their own row rendering while the
 * trigger, the filter box, the empty state and the unresolved-id escape hatch
 * exist once.
 */
function Picker<T extends INamed>({
	value,
	onChange,
	entries,
	disabled = false,
	placeholder,
	emptyLabel,
	renderTrigger,
	renderRow,
	/** Extra entries pinned below the list, e.g. an explicit "none". */
	footer,
}: {
	value: string
	onChange: (id: string) => void
	entries: T[]
	disabled?: boolean
	placeholder: string
	emptyLabel: string
	renderTrigger: (selected: T) => React.ReactNode
	renderRow: (entry: T, isSelected: boolean) => React.ReactNode
	footer?: React.ReactNode
}) {
	const [query, setQuery] = useState("")

	const filtered = useMemo(() => searchByName(entries, query), [entries, query])
	const selected = entries.find((entry) => entry.id === value)
	// A value absent from the list is stale (deleted in Discord) or invisible to the
	// bot. Either way the admin must be able to keep or clear it, never have the
	// form silently reassign it.
	const isUnresolved = value.length > 0 && selected === undefined

	const close = (id: string) => {
		onChange(id)
		setQuery("")
	}

	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild disabled={disabled}>
				<Button
					type="button"
					variant="outline"
					disabled={disabled}
					className="h-9 w-full justify-between font-normal"
				>
					<span className="truncate">
						{selected !== undefined ? (
							renderTrigger(selected)
						) : isUnresolved ? (
							<span className="font-mono text-xs">{value} (not visible to the bot)</span>
						) : (
							<span className="text-muted-foreground">{placeholder}</span>
						)}
					</span>
					<ChevronDown className="size-4 shrink-0 opacity-50" />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent
				align="start"
				className="w-(--radix-dropdown-menu-trigger-width) min-w-64 p-0"
			>
				<div className="border-b border-border p-2">
					<div className="relative">
						<Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
						<Input
							// The list opens only on an explicit click, so focusing the
							// filter is the expected behaviour rather than a surprise.
							autoFocus
							value={query}
							onChange={(event) => setQuery(event.target.value)}
							placeholder="Search by name"
							className="h-8 pl-8"
							// Radix treats keystrokes inside the content as type-ahead and
							// would jump the highlight around as the admin types; the filter
							// box is the intended target, so the event stops here.
							onKeyDown={(event) => event.stopPropagation()}
						/>
					</div>
				</div>

				<div className="max-h-64 overflow-y-auto p-1">
					{filtered.length === 0 ? (
						<p className="px-2 py-6 text-center text-xs text-muted-foreground">{emptyLabel}</p>
					) : (
						filtered.map((entry) => (
							<DropdownMenuItem key={entry.id} onSelect={() => close(entry.id)} className="gap-2">
								{renderRow(entry, entry.id === value)}
							</DropdownMenuItem>
						))
					)}
				</div>

				{isUnresolved ? (
					<div className="border-t border-border p-1">
						<DropdownMenuItem onSelect={() => setQuery("")} className="gap-2 text-xs">
							<Check className="size-3.5 shrink-0" />
							<span className="truncate">Keep {value}</span>
						</DropdownMenuItem>
					</div>
				) : null}

				{footer}
			</DropdownMenuContent>
		</DropdownMenu>
	)
}

/* -------------------------------------------------------------------------- */
/* Role picker                                                                */
/* -------------------------------------------------------------------------- */

export function RolePicker({
	value,
	onChange,
	roles,
	disabled = false,
	placeholder = "Pick a role",
	/** Role ids the ladder owns, hidden because they are managed elsewhere. */
	exclude = [],
}: {
	value: string
	onChange: (roleId: string) => void
	roles: IDirectoryRole[]
	disabled?: boolean
	placeholder?: string
	exclude?: string[]
}) {
	const options = useMemo(
		() => roles.filter((role) => !exclude.includes(role.id)),
		[roles, exclude],
	)

	return (
		<Picker
			value={value}
			onChange={onChange}
			entries={options}
			disabled={disabled}
			placeholder={placeholder}
			emptyLabel={
				options.length === 0 ? "No roles are visible to the bot." : "No role matches that name."
			}
			renderTrigger={(role) => (
				<span className="flex items-center gap-2">
					<RoleSwatch color={role.color} />@{role.name}
				</span>
			)}
			renderRow={(role, isSelected) => (
				<>
					<Check className={cn("size-3.5 shrink-0", isSelected ? "opacity-100" : "opacity-0")} />
					<RoleSwatch color={role.color} />
					<span className="truncate">{role.name}</span>
					{/* Surfaced so an admin does not pick a bot's own role and then wonder why
					    granting it does nothing. */}
					{role.managed ? (
						<span className="ml-auto shrink-0 text-[10px] text-muted-foreground">managed</span>
					) : null}
				</>
			)}
		/>
	)
}

/** A role's Discord colour, or a neutral dot when it has none set. */
export function RoleSwatch({ color }: { color: number }) {
	const hex = color === 0 ? null : `#${color.toString(16).padStart(6, "0")}`
	return (
		<span
			aria-hidden="true"
			className={cn(
				"size-2.5 shrink-0 rounded-full border",
				hex === null ? "border-muted-foreground/40" : "border-transparent",
			)}
			style={hex === null ? undefined : { backgroundColor: hex }}
		/>
	)
}

/* -------------------------------------------------------------------------- */
/* Channel picker                                                             */
/* -------------------------------------------------------------------------- */

export function ChannelPicker({
	value,
	onChange,
	channels,
	disabled = false,
	placeholder = "Pick a channel",
	allowNone = false,
	noneLabel = "No channel",
}: {
	value: string
	onChange: (channelId: string) => void
	channels: IDirectoryChannel[]
	disabled?: boolean
	placeholder?: string
	/** Offer an explicit "none" entry, for settings that are genuinely optional. */
	allowNone?: boolean
	noneLabel?: string
}) {
	return (
		<Picker
			value={value}
			onChange={onChange}
			entries={channels}
			disabled={disabled}
			placeholder={placeholder}
			emptyLabel={
				channels.length === 0
					? "No channels are visible to the bot."
					: "No channel matches that name."
			}
			renderTrigger={(channel) => (
				<span className="flex items-center gap-1.5">
					<Hash className="size-3.5 shrink-0 text-muted-foreground" />
					{channel.name}
				</span>
			)}
			renderRow={(channel, isSelected) => (
				<>
					<Check className={cn("size-3.5 shrink-0", isSelected ? "opacity-100" : "opacity-0")} />
					<Hash className="size-3.5 shrink-0 text-muted-foreground" />
					<span className="truncate">{channel.name}</span>
				</>
			)}
			footer={
				allowNone ? (
					<div className="border-t border-border p-1">
						<DropdownMenuItem onSelect={() => onChange("")} className="gap-2">
							<Check
								className={cn("size-3.5 shrink-0", value === "" ? "opacity-100" : "opacity-0")}
							/>
							<span className="text-muted-foreground">{noneLabel}</span>
						</DropdownMenuItem>
					</div>
				) : null
			}
		/>
	)
}

/**
 * A role rendered as a name, with the role's colour beside it.
 *
 * When the role cannot be resolved — deleted in Discord, or invisible to the bot —
 * the short id is shown instead. That fallback matters: the row still points at a
 * real snowflake, and hiding it behind a blank cell would make a misconfigured
 * entry look like an empty one.
 */
export function RoleName({
	roleId,
	rolesById,
	className,
}: {
	roleId: string
	rolesById: Map<string, IDirectoryRole>
	className?: string
}) {
	const role = rolesById.get(roleId)

	if (role === undefined) {
		return (
			<span
				className={cn("font-mono text-xs text-muted-foreground", className)}
				title="Not visible to the bot"
			>
				{shortId(roleId)}
			</span>
		)
	}

	return (
		<span className={cn("flex items-center gap-1.5", className)}>
			<RoleSwatch color={role.color} />@{role.name}
		</span>
	)
}

/**
 * A warning strip for when names could not be read at all.
 *
 * The dashboard still works with ids — they are what the API stores — so this
 * explains the degraded state instead of showing an error where a picker belongs.
 */
export function DirectoryUnavailableNotice({ className }: { className?: string }) {
	return (
		<p className={cn("flex items-center gap-1.5 text-xs text-muted-foreground", className)}>
			<ShieldAlert className="size-3.5 shrink-0" />
			Role and channel names are unavailable, so ids are shown instead.
		</p>
	)
}
