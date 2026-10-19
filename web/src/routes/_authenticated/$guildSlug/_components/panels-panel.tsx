import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { MousePointerClick, Pencil, Plus, Send, Trash2 } from "lucide-react"
import { useState } from "react"
import { toast } from "sonner"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { FormField } from "#/components/features/form-field.tsx"
import { PermissionButton } from "#/components/features/permission-button.tsx"
import { FormError } from "#/components/features/query-boundary.tsx"
import { Badge } from "#/components/ui/badge.tsx"
import { Button } from "#/components/ui/button.tsx"
import { Card, CardContent, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "#/components/ui/dialog.tsx"
import { Input } from "#/components/ui/input.tsx"
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "#/components/ui/select.tsx"
import { panelsApi } from "#/libs/api/resources.ts"
import type {
	IPanelCreateRequest,
	IPanelResponse,
	IPanelSummaryResponse,
} from "#/libs/api/types.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/** Discord's button styles, which the API stores as an integer. */
const STYLES = [
	{ value: 2, label: "Secondary (grey)" },
	{ value: 1, label: "Primary (blurple)" },
	{ value: 3, label: "Success (green)" },
	{ value: 4, label: "Danger (red)" },
]

const BEHAVIOURS = [
	{ value: "toggle", label: "Toggle — pressing again removes the role" },
	{ value: "unique", label: "Exclusive — removes the panel's other roles" },
]

/**
 * One panel, with its buttons managed in place.
 *
 * Not a card that links to a detail page: the buttons are what an admin actually
 * edits, and a guild has only a handful of panels.
 *
 * There is deliberately no delete control. The API has no delete for a panel — the
 * panel *is* a Discord message, so removing it means deleting that message in
 * Discord. A button that quietly did nothing would be worse than no button.
 */
function PanelCard({
	panel,
	guildId,
	canWrite,
	deniedReason,
}: {
	panel: IPanelSummaryResponse
	guildId: string
	canWrite: boolean
	deniedReason: string
}) {
	const queryClient = useQueryClient()
	const detail = useQuery(guildQueries.panel(guildId, panel.id))
	const [optionOpen, setOptionOpen] = useState(false)
	const [publishing, setPublishing] = useState(false)
	const [editing, setEditing] = useState(false)

	const full = detail.data as IPanelResponse | undefined
	const options = full?.options ?? []

	const invalidate = () => {
		void queryClient.invalidateQueries({ queryKey: queryKeys.panels.all(guildId) })
	}

	const addOption = useMutation({
		mutationFn: (body: Parameters<typeof panelsApi.createOption>[2]) =>
			panelsApi.createOption(guildId, panel.id, body),
		onSuccess: () => {
			invalidate()
			setOptionOpen(false)
			toast.success("Button added")
		},
	})

	const removeOption = useMutation({
		mutationFn: (optionId: string) => panelsApi.deleteOption(guildId, panel.id, optionId),
		onSuccess: () => {
			invalidate()
			toast.success("Button removed")
		},
	})

	const publish = useMutation({
		mutationFn: (body: { channel_id: string }) => panelsApi.publish(guildId, panel.id, body),
		onSuccess: () => {
			invalidate()
			setPublishing(false)
			toast.success("Panel posted")
		},
	})

	const rename = useMutation({
		mutationFn: (body: { slug: string; title: string }) =>
			panelsApi.update(guildId, panel.id, body),
		onSuccess: () => {
			invalidate()
			setEditing(false)
			toast.success("Panel updated")
		},
	})

	// Discord rejects a message with more than 25 components, so the API refuses the
	// 26th button. Saying so here beats letting the press fail.
	const atCapacity = panel.option_count >= 25

	return (
		<Card>
			<CardHeader className="flex flex-row items-start justify-between gap-3">
				<div className="flex flex-col gap-1">
					<CardTitle className="flex items-center gap-2">
						{panel.title}
						{panel.is_published ? (
							<Badge variant="secondary">Published</Badge>
						) : (
							<Badge variant="outline">Draft</Badge>
						)}
					</CardTitle>
					<p className="font-mono text-xs text-muted-foreground">{panel.slug}</p>
				</div>
				<div className="flex items-center gap-1">
					<PermissionButton
						allowed={canWrite}
						reason={deniedReason}
						variant="ghost"
						size="sm"
						onClick={() => setEditing(true)}
					>
						<Pencil className="size-4" />
						Edit
					</PermissionButton>
					<PermissionButton
						allowed={canWrite}
						reason={deniedReason}
						variant="ghost"
						size="sm"
						onClick={() => setPublishing(true)}
					>
						<Send className="size-4" />
						Publish
					</PermissionButton>
				</div>
			</CardHeader>

			<CardContent className="flex flex-col gap-3">
				<p className="text-sm text-muted-foreground">
					{panel.option_count === 0
						? "No buttons yet. A panel with no buttons cannot be published."
						: `${panel.option_count} of 25 ${panel.option_count === 1 ? "button" : "buttons"} used.`}
				</p>

				{detail.isError ? <FormError error={detail.error} /> : null}

				{options.length > 0 ? (
					<ul className="flex flex-col divide-y divide-border rounded-md border border-border">
						{options.map((option) => (
							<li
								key={option.id}
								className="flex items-center justify-between gap-3 px-3 py-2 text-sm"
							>
								<span className="flex items-center gap-2">
									{option.emoji ? <span>{option.emoji}</span> : null}
									<span className="font-medium">{option.label}</span>
									<span className="font-mono text-xs text-muted-foreground">{option.role_id}</span>
									<Badge variant="outline">{option.behavior}</Badge>
								</span>
								<Button
									variant="ghost"
									size="sm"
									disabled={!canWrite || removeOption.isPending}
									onClick={() => removeOption.mutate(option.id)}
									aria-label={`Remove ${option.label}`}
								>
									<Trash2 className="size-4" />
								</Button>
							</li>
						))}
					</ul>
				) : null}

				{removeOption.error ? <FormError error={removeOption.error} /> : null}

				<div>
					<PermissionButton
						allowed={canWrite}
						reason={deniedReason}
						variant="outline"
						size="sm"
						disabled={atCapacity}
						onClick={() => setOptionOpen(true)}
					>
						<Plus className="size-4" />
						Add button
					</PermissionButton>
					{atCapacity ? (
						<p className="mt-1 text-xs text-muted-foreground">
							This panel already holds Discord&rsquo;s limit of 25 buttons.
						</p>
					) : null}
				</div>
			</CardContent>

			<OptionDialog
				open={optionOpen}
				onOpenChange={setOptionOpen}
				pending={addOption.isPending}
				error={addOption.error}
				onSubmit={(body) => addOption.mutate(body)}
			/>

			<PublishDialog
				open={publishing}
				onOpenChange={setPublishing}
				pending={publish.isPending}
				error={publish.error}
				defaultChannel={panel.channel_id ?? ""}
				onSubmit={(body) => publish.mutate(body)}
			/>

			<RenameDialog
				open={editing}
				onOpenChange={setEditing}
				pending={rename.isPending}
				error={rename.error}
				slug={panel.slug}
				title={panel.title}
				onSubmit={(body) => rename.mutate(body)}
			/>
		</Card>
	)
}

/** Adds one button to a panel. */
function OptionDialog({
	open,
	onOpenChange,
	pending,
	error,
	onSubmit,
}: {
	open: boolean
	onOpenChange: (open: boolean) => void
	pending: boolean
	error: unknown
	onSubmit: (body: {
		label: string
		role_id: string
		emoji: string | null
		style: number
		behavior: string
	}) => void
}) {
	const [label, setLabel] = useState("")
	const [roleId, setRoleId] = useState("")
	const [emoji, setEmoji] = useState("")
	const [style, setStyle] = useState(2)
	const [behavior, setBehavior] = useState("toggle")

	const reset = () => {
		setLabel("")
		setRoleId("")
		setEmoji("")
		setStyle(2)
		setBehavior("toggle")
	}

	return (
		<Dialog
			open={open}
			onOpenChange={(next) => {
				if (!next) reset()
				onOpenChange(next)
			}}
		>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>Add a button</DialogTitle>
					<DialogDescription>
						One button grants one Discord role. A panel holds at most 25, which is Discord&rsquo;s
						limit per message.
					</DialogDescription>
				</DialogHeader>

				<div className="flex flex-col gap-4">
					<FormField label="Label" htmlFor="option-label">
						<Input
							id="option-label"
							value={label}
							onChange={(event) => setLabel(event.target.value)}
							placeholder="Announcements"
						/>
					</FormField>

					<FormField label="Role id" htmlFor="option-role">
						<Input
							id="option-role"
							value={roleId}
							onChange={(event) => setRoleId(event.target.value)}
							placeholder="456789012345678901"
						/>
					</FormField>

					<FormField label="Emoji" htmlFor="option-emoji" description="Optional.">
						<Input
							id="option-emoji"
							value={emoji}
							onChange={(event) => setEmoji(event.target.value)}
							placeholder="📢"
						/>
					</FormField>

					<FormField label="Style" htmlFor="option-style">
						<Select
							value={String(style)}
							onValueChange={(value) => setStyle(Number.parseInt(value, 10))}
						>
							<SelectTrigger id="option-style">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{STYLES.map((entry) => (
									<SelectItem key={entry.value} value={String(entry.value)}>
										{entry.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</FormField>

					<FormField label="Behaviour" htmlFor="option-behaviour">
						<Select value={behavior} onValueChange={setBehavior}>
							<SelectTrigger id="option-behaviour">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{BEHAVIOURS.map((entry) => (
									<SelectItem key={entry.value} value={entry.value}>
										{entry.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</FormField>

					{error ? <FormError error={error} /> : null}
				</div>

				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						Cancel
					</Button>
					<Button
						disabled={pending || !label.trim() || !roleId.trim()}
						onClick={() =>
							onSubmit({
								label: label.trim(),
								role_id: roleId.trim(),
								emoji: emoji.trim() || null,
								style,
								behavior,
							})
						}
					>
						Add button
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	)
}

/**
 * Posts the panel to a channel.
 *
 * One field, because posting is the API's job: it holds the bot token, gets a
 * message id back, and records it. Asking the admin for a message id was asking
 * them to do the bot's work, which is why nothing was ever posted.
 */
function PublishDialog({
	open,
	onOpenChange,
	pending,
	error,
	defaultChannel,
	onSubmit,
}: {
	open: boolean
	onOpenChange: (open: boolean) => void
	pending: boolean
	error: unknown
	defaultChannel: string
	onSubmit: (body: { channel_id: string }) => void
}) {
	const [channel, setChannel] = useState(defaultChannel)

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>Post this panel</DialogTitle>
					<DialogDescription>
						The bot posts the panel to the channel you name and remembers the message, so publishing
						again edits it instead of posting a second copy.
					</DialogDescription>
				</DialogHeader>

				<div className="flex flex-col gap-4">
					<FormField
						label="Channel id"
						htmlFor="publish-channel"
						description="The bot needs Send Messages there."
					>
						<Input
							id="publish-channel"
							value={channel}
							onChange={(event) => setChannel(event.target.value)}
							placeholder="123456789012345678"
							inputMode="numeric"
							className="font-mono"
						/>
					</FormField>
					{error ? <FormError error={error} /> : null}
				</div>

				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						Cancel
					</Button>
					<Button
						disabled={pending || !channel.trim()}
						onClick={() => onSubmit({ channel_id: channel.trim() })}
					>
						Post panel
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	)
}

/** Rewrites a panel's title. The slug is fixed because it addresses the panel. */
function RenameDialog({
	open,
	onOpenChange,
	pending,
	error,
	slug,
	title,
	onSubmit,
}: {
	open: boolean
	onOpenChange: (open: boolean) => void
	pending: boolean
	error: unknown
	slug: string
	title: string
	onSubmit: (body: { slug: string; title: string }) => void
}) {
	const [value, setValue] = useState(title)

	return (
		<Dialog
			open={open}
			onOpenChange={(next) => {
				if (next) setValue(title)
				onOpenChange(next)
			}}
		>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>Edit panel</DialogTitle>
					<DialogDescription>
						The slug <span className="font-mono">{slug}</span> addresses this panel and cannot
						change; the title is what members see.
					</DialogDescription>
				</DialogHeader>

				<div className="flex flex-col gap-4">
					<FormField label="Title" htmlFor="rename-title">
						<Input
							id="rename-title"
							value={value}
							onChange={(event) => setValue(event.target.value)}
						/>
					</FormField>
					{error ? <FormError error={error} /> : null}
				</div>

				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						Cancel
					</Button>
					<Button
						disabled={pending || !value.trim()}
						onClick={() => onSubmit({ slug, title: value.trim() })}
					>
						Save
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	)
}

/** Creates a panel header; buttons are added afterwards. */
function NewPanelDialog({
	open,
	onOpenChange,
	pending,
	error,
	onSubmit,
}: {
	open: boolean
	onOpenChange: (open: boolean) => void
	pending: boolean
	error: unknown
	onSubmit: (body: IPanelCreateRequest) => void
}) {
	const [slug, setSlug] = useState("")
	const [title, setTitle] = useState("")
	const [description, setDescription] = useState("")

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>New role panel</DialogTitle>
					<DialogDescription>
						A panel is one Discord message of self-assign buttons. The slug is how it is addressed,
						so it has to be unique in this server.
					</DialogDescription>
				</DialogHeader>

				<div className="flex flex-col gap-4">
					<FormField label="Slug" htmlFor="panel-slug" description="Lowercase, no spaces.">
						<Input
							id="panel-slug"
							value={slug}
							onChange={(event) => setSlug(event.target.value)}
							placeholder="self-roles"
						/>
					</FormField>
					<FormField label="Title" htmlFor="panel-title">
						<Input
							id="panel-title"
							value={title}
							onChange={(event) => setTitle(event.target.value)}
							placeholder="Pick your roles"
						/>
					</FormField>
					<FormField label="Description" htmlFor="panel-description" description="Optional.">
						<Input
							id="panel-description"
							value={description}
							onChange={(event) => setDescription(event.target.value)}
							placeholder="Choose the roles you want."
						/>
					</FormField>
					{error ? <FormError error={error} /> : null}
				</div>

				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						Cancel
					</Button>
					<Button
						disabled={pending || !slug.trim() || !title.trim()}
						onClick={() =>
							onSubmit({
								slug: slug.trim(),
								title: title.trim(),
								description: description.trim() || null,
							})
						}
					>
						Create panel
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	)
}

/** The guild's role panels. */
export function PanelsPanel({
	guildId,
	canWrite,
	deniedReason,
}: {
	guildId: string
	canWrite: boolean
	deniedReason: string
}) {
	const queryClient = useQueryClient()
	const query = useQuery(guildQueries.panels(guildId, {}))
	const panels = query.data?.data ?? []

	const [creating, setCreating] = useState(false)

	const create = useMutation({
		mutationFn: (body: IPanelCreateRequest) => panelsApi.create(guildId, body),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: queryKeys.panels.all(guildId) })
			setCreating(false)
			toast.success("Panel created")
		},
	})

	return (
		<div className="flex flex-col gap-6">
			<div className="flex items-center justify-between gap-3">
				<p className="text-sm text-muted-foreground">
					{panels.length === 0
						? "No panels yet."
						: `${panels.length} ${panels.length === 1 ? "panel" : "panels"}.`}
				</p>
				<PermissionButton
					allowed={canWrite}
					reason={deniedReason}
					onClick={() => setCreating(true)}
				>
					<Plus className="size-4" />
					New panel
				</PermissionButton>
			</div>

			{query.isError ? <FormError error={query.error} /> : null}

			{panels.length === 0 && !query.isPending ? (
				<EmptyState
					icon={MousePointerClick}
					title="No role panels yet"
					description="A panel is a message members click to give themselves roles — no moderator needed."
					action={
						<PermissionButton
							allowed={canWrite}
							reason={deniedReason}
							onClick={() => setCreating(true)}
						>
							Create the first panel
						</PermissionButton>
					}
				/>
			) : null}

			<div className="flex flex-col gap-4">
				{panels.map((panel) => (
					<PanelCard
						key={panel.id}
						panel={panel}
						guildId={guildId}
						canWrite={canWrite}
						deniedReason={deniedReason}
					/>
				))}
			</div>

			<NewPanelDialog
				open={creating}
				onOpenChange={setCreating}
				pending={create.isPending}
				error={create.error}
				onSubmit={(body) => create.mutate(body)}
			/>
		</div>
	)
}
