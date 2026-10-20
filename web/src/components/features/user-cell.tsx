import { Avatar, AvatarFallback, AvatarImage } from "#/components/ui/avatar.tsx"
import { avatarUrl, cn, initialsOf } from "#/libs/utils.ts"

/**
 * Avatar + name + id, in one consistent block. Used by the member list, the
 * economy account lookup, both leaderboards and the transaction log, so a
 * snowflake is always rendered the same way.
 *
 * With no `name` the id is the headline. It is ugly, and that is the point: the
 * alternative the dashboard used to render — "User 1504…8976" — reads as a real
 * member whose name nobody can look up, which is a worse lie than a snowflake. A
 * name is either something the bot actually observed or it is not a name.
 */
export function UserCell({
	discordId,
	name,
	avatar,
	secondary,
	className,
}: {
	discordId: string
	name?: string | null
	avatar?: string | null
	/** Overrides the id line, e.g. a role name or a level. */
	secondary?: React.ReactNode
	className?: string
}) {
	const displayName = name ?? discordId
	const src = avatarUrl(avatar, 64)

	return (
		<div className={cn("flex min-w-0 items-center gap-3", className)}>
			<Avatar className="size-8 shrink-0">
				{src !== null ? <AvatarImage src={src} alt="" /> : null}
				<AvatarFallback className="text-[11px]">{initialsOf(displayName)}</AvatarFallback>
			</Avatar>
			<div className="min-w-0">
				<p className="truncate text-sm font-medium">{displayName}</p>
				<p className="truncate font-mono text-xs text-muted-foreground">{secondary ?? discordId}</p>
			</div>
		</div>
	)
}
