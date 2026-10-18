import { useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { LogOut, ShieldAlert, UserRound } from "lucide-react"
import { toast } from "sonner"
import { Avatar, AvatarFallback, AvatarImage } from "#/components/ui/avatar.tsx"
import { Button } from "#/components/ui/button.tsx"
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "#/components/ui/dropdown-menu.tsx"
import { useCurrentUser } from "#/hooks/use-me.ts"
import { useSession } from "#/libs/auth/session-context.tsx"
import { avatarUrl, initialsOf } from "#/libs/utils.ts"

/**
 * Identity block at the foot of the sidebar: who you are, your super-admin
 * badge if you have one, and the two sign-out flavours the API supports.
 */
export function UserMenu() {
	const user = useCurrentUser()
	const { signOut } = useSession()
	const navigate = useNavigate()
	const queryClient = useQueryClient()

	const handleSignOut = async (everywhere: boolean) => {
		await signOut({ everywhere })
		// Drop every cached guild payload: another account may sign in next.
		queryClient.clear()
		toast.success(everywhere ? "Signed out on all devices" : "Signed out")
		await navigate({ to: "/", replace: true })
	}

	const displayName = user?.global_name ?? user?.username ?? "Signed in"
	const avatar = avatarUrl(user?.avatar_url, 64)

	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button
					variant="ghost"
					className="h-auto w-full justify-start gap-2 px-2 py-2 hover:bg-sidebar-accent"
					aria-label="Account menu"
				>
					<Avatar className="size-7">
						{avatar !== null ? <AvatarImage src={avatar} alt="" /> : null}
						<AvatarFallback className="text-[11px]">{initialsOf(displayName)}</AvatarFallback>
					</Avatar>
					<span className="min-w-0 flex-1 text-left">
						<span className="block truncate text-sm font-medium">{displayName}</span>
						<span className="block truncate text-xs text-muted-foreground">
							{user ? `@${user.username}` : "—"}
						</span>
					</span>
				</Button>
			</DropdownMenuTrigger>

			<DropdownMenuContent align="start" side="top" className="w-60">
				<DropdownMenuLabel className="flex items-center gap-2">
					<UserRound className="size-3.5" />
					<span className="truncate">{displayName}</span>
				</DropdownMenuLabel>
				{user?.is_super_admin ? (
					<>
						<DropdownMenuSeparator />
						<div className="flex items-center gap-2 px-2 py-1.5 text-xs text-muted-foreground">
							<ShieldAlert className="size-3.5 text-warning" />
							Super admin
						</div>
					</>
				) : null}
				<DropdownMenuSeparator />
				<DropdownMenuItem onSelect={() => void handleSignOut(false)}>
					<LogOut className="size-4" />
					Sign out
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={() => void handleSignOut(true)}>
					<LogOut className="size-4" />
					Sign out everywhere
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	)
}
