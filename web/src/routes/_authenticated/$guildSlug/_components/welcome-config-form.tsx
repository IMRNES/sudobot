import { useForm } from "@tanstack/react-form"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { RefreshCw } from "lucide-react"
import { toast } from "sonner"
import { ChannelPicker } from "#/components/features/discord-pickers.tsx"
import { FormField, FormRow } from "#/components/features/form-field.tsx"
import { PermissionButton } from "#/components/features/permission-button.tsx"
import { QueryBoundary } from "#/components/features/query-boundary.tsx"
import { Button } from "#/components/ui/button.tsx"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { Input } from "#/components/ui/input.tsx"
import { Label } from "#/components/ui/label.tsx"
import { Switch } from "#/components/ui/switch.tsx"
import { Textarea } from "#/components/ui/textarea.tsx"
import { announceableChannels, useGuildDirectory } from "#/hooks/use-guild-directory.ts"
import { welcomeApi } from "#/libs/api/resources.ts"
import {
	type IWelcomeConfigResponse,
	type IWelcomePreviewResponse,
	WELCOME_PLACEHOLDERS,
} from "#/libs/api/types.ts"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { hexColorToNumber, numberToHexColor } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * Welcome and goodbye message configuration.
 *
 * Message templates are validated against the API's own allow-list
 * (`{user}`, `{mention}`, `{guild}`, `{memberCount}`) before the request goes
 * out — API.md states anything else is a `400`, and catching it here means the
 * admin sees which placeholder is wrong instead of one opaque rejection.
 */
export function WelcomeConfigForm({
	guildId,
	canWrite,
	deniedReason,
}: {
	guildId: string
	canWrite: boolean
	deniedReason: string
}) {
	const query = useQuery(guildQueries.welcome(guildId))

	return (
		<Card>
			<CardHeader>
				<CardTitle className="text-base">Messages</CardTitle>
				<CardDescription>
					Sent when somebody joins or leaves. Placeholders are replaced per member.
				</CardDescription>
			</CardHeader>
			<CardContent>
				<QueryBoundary isLoading={query.isLoading} isError={query.isError} error={query.error}>
					{query.data ? (
						<WelcomeForm
							key={JSON.stringify(query.data)}
							guildId={guildId}
							config={query.data}
							canWrite={canWrite}
							deniedReason={deniedReason}
						/>
					) : null}
				</QueryBoundary>
			</CardContent>
		</Card>
	)
}

/** Reject any `{...}` token the API would reject, before sending. */
export function findUnknownPlaceholder(message: string): string | null {
	const matches = message.matchAll(/\{([^}]*)\}/g)
	const allowed = new Set<string>(WELCOME_PLACEHOLDERS)
	for (const match of matches) {
		const token = `{${match[1]}}`
		if (!allowed.has(token)) return token
	}
	return null
}

function WelcomeForm({
	guildId,
	config,
	canWrite,
	deniedReason,
}: {
	guildId: string
	config: IWelcomeConfigResponse
	canWrite: boolean
	deniedReason: string
}) {
	const queryClient = useQueryClient()

	// Channel names for the two channel fields below. Without this the form asks an
	// admin to paste a snowflake, which is the work of reading Discord handed to them.
	// Filtered to the types the bot can actually post to, so a category or a voice
	// room is never offered as a destination.
	const { channels, isLoading: namesLoading } = useGuildDirectory(guildId)
	const postable = announceableChannels(channels)

	const save = useMutation({
		mutationFn: welcomeApi.update.bind(null, guildId),
		onSuccess: (updated) => {
			queryClient.setQueryData(queryKeys.welcome.config(guildId), updated)
			// The preview is derived from the saved config, so it is stale the moment
			// a save lands — and a stale preview of a message template is exactly the
			// thing an admin would trust and be wrong about.
			void queryClient.invalidateQueries({ queryKey: queryKeys.welcome.preview(guildId) })
			toast.success("Welcome settings saved")
		},
		onError: (error) => {
			const normalised = normalizeApiError(error)
			toast.error("Could not save welcome settings", { description: normalised.message })
		},
	})

	const form = useForm({
		defaultValues: {
			welcome_enabled: config.welcome_enabled,
			welcome_channel_id: config.welcome_channel_id ?? "",
			welcome_message: config.welcome_message ?? "",
			welcome_embed_color: numberToHexColor(config.welcome_embed_color) ?? "#5865f2",
			welcome_dm_enabled: config.welcome_dm_enabled,
			embed_title: config.embed_title ?? "",
			embed_description: config.embed_description ?? "",
			embed_author: config.embed_author ?? "",
			embed_image_url: config.embed_image_url ?? "",
			embed_thumbnail_url: config.embed_thumbnail_url ?? "",
			goodbye_enabled: config.goodbye_enabled,
			goodbye_channel_id: config.goodbye_channel_id ?? "",
			goodbye_message: config.goodbye_message ?? "",
			goodbye_embed_color: numberToHexColor(config.goodbye_embed_color) ?? "#ed4245",
		},
		onSubmit: ({ value }) => {
			save.mutate({
				welcome_enabled: value.welcome_enabled,
				welcome_channel_id: value.welcome_channel_id.trim() || null,
				welcome_message: value.welcome_message.trim() || null,
				welcome_embed_color: hexColorToNumber(value.welcome_embed_color),
				welcome_dm_enabled: value.welcome_dm_enabled,
				// The card is shared, and an empty field means "clear this part".
				embed_title: value.embed_title.trim() || null,
				embed_description: value.embed_description.trim() || null,
				embed_author: value.embed_author.trim() || null,
				embed_image_url: value.embed_image_url.trim() || null,
				embed_thumbnail_url: value.embed_thumbnail_url.trim() || null,
				goodbye_enabled: value.goodbye_enabled,
				goodbye_channel_id: value.goodbye_channel_id.trim() || null,
				goodbye_message: value.goodbye_message.trim() || null,
				goodbye_embed_color: hexColorToNumber(value.goodbye_embed_color),
			})
		},
	})

	return (
		<form
			className="flex flex-col gap-6"
			onSubmit={(event) => {
				event.preventDefault()
				event.stopPropagation()
				if (!canWrite) return
				void form.handleSubmit()
			}}
		>
			{/* ------------------------------------------------ welcome message */}

			<form.Field name="welcome_enabled">
				{(field) => (
					<ToggleRow
						id="welcome-enabled"
						label="Welcome messages"
						description="Post a message when somebody joins the server."
						checked={field.state.value}
						disabled={!canWrite}
						reason={deniedReason}
						onChange={(checked) => field.handleChange(checked)}
					/>
				)}
			</form.Field>

			<form.Subscribe selector={(state) => state.values.welcome_enabled}>
				{(enabled) =>
					!enabled ? null : (
						<div className="flex flex-col gap-4 border-l-2 border-border pl-4">
							<FormRow>
								<form.Field name="welcome_channel_id">
									{(field) => (
										<FormField
											label="Channel"
											htmlFor={field.name}
											description="Where the message is posted."
										>
											<ChannelPicker
												value={field.state.value}
												onChange={field.handleChange}
												channels={postable}
												isLoading={namesLoading}
												disabled={!canWrite}
												placeholder="No channel"
												allowNone
											/>
										</FormField>
									)}
								</form.Field>

								<form.Field name="welcome_embed_color">
									{(field) => (
										<FormField label="Embed colour" htmlFor={field.name}>
											<div className="flex items-center gap-2">
												<input
													id={field.name}
													type="color"
													value={field.state.value}
													disabled={!canWrite}
													onChange={(event) => field.handleChange(event.target.value)}
													className="h-9 w-12 shrink-0 cursor-pointer rounded-md border border-input bg-transparent disabled:cursor-not-allowed"
													aria-label="Embed colour"
												/>
												<Input
													value={field.state.value}
													disabled={!canWrite}
													onChange={(event) => field.handleChange(event.target.value)}
													className="font-mono"
												/>
											</div>
										</FormField>
									)}
								</form.Field>
							</FormRow>

							<form.Field
								name="welcome_message"
								validators={{
									onChange: ({ value }) => {
										const unknown = findUnknownPlaceholder(value)
										return unknown === null
											? undefined
											: `Unknown placeholder ${unknown}. Allowed: ${WELCOME_PLACEHOLDERS.join(", ")}.`
									},
								}}
							>
								{(field) => (
									<FormField
										label="Message"
										htmlFor={field.name}
										error={field.state.meta.errors[0]?.toString()}
									>
										<Textarea
											id={field.name}
											value={field.state.value}
											disabled={!canWrite}
											onBlur={field.handleBlur}
											onChange={(event) => field.handleChange(event.target.value)}
											placeholder="Welcome {mention} to {guild}! You are member #{memberCount}."
											rows={3}
										/>
										<PlaceholderHints />
									</FormField>
								)}
							</form.Field>

							<form.Field name="welcome_dm_enabled">
								{(field) => (
									<ToggleRow
										id="welcome-dm"
										label="Also send a direct message"
										description="Sends the same text to the member privately."
										checked={field.state.value}
										disabled={!canWrite}
										reason={deniedReason}
										onChange={(checked) => field.handleChange(checked)}
									/>
								)}
							</form.Field>
						</div>
					)
				}
			</form.Subscribe>

			{/* ------------------------------------------------ the embed card */}

			<div className="flex flex-col gap-4 border-t border-border pt-6">
				<div>
					<h3 className="text-sm font-medium">Server card</h3>
					<p className="text-xs text-muted-foreground">
						The embed that appears under every welcome and goodbye. Shared by both, so the server's
						branding is written once.
					</p>
				</div>

				<FormRow>
					<form.Field name="embed_author">
						{(field) => (
							<FormField
								label="Author line"
								htmlFor={field.name}
								description="Small grey text above the title."
							>
								<Input
									id={field.name}
									value={field.state.value}
									disabled={!canWrite}
									placeholder="imrnes.team"
									onBlur={field.handleBlur}
									onChange={(event) => field.handleChange(event.target.value)}
								/>
							</FormField>
						)}
					</form.Field>

					<form.Field name="embed_title">
						{(field) => (
							<FormField label="Title" htmlFor={field.name} description="Shown bold.">
								<Input
									id={field.name}
									value={field.state.value}
									disabled={!canWrite}
									placeholder="IMRNES Community"
									onBlur={field.handleBlur}
									onChange={(event) => field.handleChange(event.target.value)}
								/>
							</FormField>
						)}
					</form.Field>
				</FormRow>

				<form.Field name="embed_description">
					{(field) => (
						<FormField
							label="Description"
							htmlFor={field.name}
							description="The body of the card. Placeholders work here too."
						>
							<Input
								id={field.name}
								value={field.state.value}
								disabled={!canWrite}
								onBlur={field.handleBlur}
								onChange={(event) => field.handleChange(event.target.value)}
							/>
						</FormField>
					)}
				</form.Field>

				<FormRow>
					<form.Field name="embed_image_url">
						{(field) => (
							<FormField
								label="Image URL"
								htmlFor={field.name}
								description="The large picture under the description."
							>
								<Input
									id={field.name}
									value={field.state.value}
									disabled={!canWrite}
									placeholder="https://…/card.jpg"
									onBlur={field.handleBlur}
									onChange={(event) => field.handleChange(event.target.value)}
								/>
							</FormField>
						)}
					</form.Field>

					<form.Field name="embed_thumbnail_url">
						{(field) => (
							<FormField
								label="Thumbnail URL"
								htmlFor={field.name}
								description="The small picture in the corner."
							>
								<Input
									id={field.name}
									value={field.state.value}
									disabled={!canWrite}
									placeholder="https://…/logo.png"
									onBlur={field.handleBlur}
									onChange={(event) => field.handleChange(event.target.value)}
								/>
							</FormField>
						)}
					</form.Field>
				</FormRow>
			</div>

			{/* ------------------------------------------------ goodbye message */}

			<form.Field name="goodbye_enabled">
				{(field) => (
					<ToggleRow
						id="goodbye-enabled"
						label="Goodbye messages"
						description="Post a message when somebody leaves."
						checked={field.state.value}
						disabled={!canWrite}
						reason={deniedReason}
						onChange={(checked) => field.handleChange(checked)}
					/>
				)}
			</form.Field>

			<form.Subscribe selector={(state) => state.values.goodbye_enabled}>
				{(enabled) =>
					!enabled ? null : (
						<div className="flex flex-col gap-4 border-l-2 border-border pl-4">
							<FormRow>
								<form.Field name="goodbye_channel_id">
									{(field) => (
										<FormField label="Channel" htmlFor={field.name}>
											<ChannelPicker
												value={field.state.value}
												onChange={field.handleChange}
												channels={postable}
												isLoading={namesLoading}
												disabled={!canWrite}
												placeholder="No channel"
												allowNone
											/>
										</FormField>
									)}
								</form.Field>

								<form.Field name="goodbye_embed_color">
									{(field) => (
										<FormField label="Embed colour" htmlFor={field.name}>
											<div className="flex items-center gap-2">
												<input
													id={field.name}
													type="color"
													value={field.state.value}
													disabled={!canWrite}
													onChange={(event) => field.handleChange(event.target.value)}
													className="h-9 w-12 shrink-0 cursor-pointer rounded-md border border-input bg-transparent disabled:cursor-not-allowed"
													aria-label="Embed colour"
												/>
												<Input
													value={field.state.value}
													disabled={!canWrite}
													onChange={(event) => field.handleChange(event.target.value)}
													className="font-mono"
												/>
											</div>
										</FormField>
									)}
								</form.Field>
							</FormRow>

							<form.Field
								name="goodbye_message"
								validators={{
									onChange: ({ value }) => {
										if (value.trim().length === 0) return undefined
										const unknown = findUnknownPlaceholder(value)
										return unknown === null
											? undefined
											: `Unknown placeholder ${unknown}. Allowed: ${WELCOME_PLACEHOLDERS.join(", ")}.`
									},
								}}
							>
								{(field) => (
									<FormField
										label="Message"
										htmlFor={field.name}
										error={field.state.meta.errors[0]?.toString()}
									>
										<Textarea
											id={field.name}
											value={field.state.value}
											disabled={!canWrite}
											onBlur={field.handleBlur}
											onChange={(event) => field.handleChange(event.target.value)}
											placeholder="{user} left {guild}."
											rows={3}
										/>
										<PlaceholderHints />
									</FormField>
								)}
							</form.Field>
						</div>
					)
				}
			</form.Subscribe>

			<WelcomePreview guildId={guildId} />

			<div className="flex justify-end">
				<PermissionButton
					type="submit"
					allowed={canWrite}
					reason={deniedReason}
					disabled={save.isPending}
				>
					{save.isPending ? "Saving…" : "Save messages"}
				</PermissionButton>
			</div>
		</form>
	)
}

/**
 * What a new member would actually see.
 *
 * The server renders it from the *saved* config, because that is what the bot
 * reads when somebody joins — showing the unsaved form would be a preview of
 * nothing. So this says "saved", and a save invalidates it.
 *
 * Worth the extra request: `{Membercount}` and `{member_count}` look fine in a
 * template and are wrong the moment a member arrives.
 */
function WelcomePreview({ guildId }: { guildId: string }) {
	const query = useQuery(guildQueries.welcomePreview(guildId))
	const rendered = query.data?.rendered ?? null
	const embed = query.data?.embed ?? null

	return (
		<div className="flex flex-col gap-2 rounded-lg border border-border bg-muted/25 p-4">
			<div className="flex items-center justify-between gap-3">
				<span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
					Preview — what a new member sees
				</span>
				<Button
					variant="ghost"
					size="sm"
					disabled={query.isFetching}
					onClick={() => void query.refetch()}
				>
					<RefreshCw className={query.isFetching ? "size-4 animate-spin" : "size-4"} />
					Refresh
				</Button>
			</div>

			{query.isError ? (
				<p className="text-xs text-destructive">{normalizeApiError(query.error).message}</p>
			) : query.isLoading ? (
				<p className="text-sm text-muted-foreground">Rendering…</p>
			) : rendered === null ? (
				<p className="text-sm text-muted-foreground">
					Nothing would be sent: the welcome message is off or empty.
				</p>
			) : (
				<div className="flex flex-col gap-3">
					<p className="text-sm break-words whitespace-pre-wrap">{rendered}</p>
					{embed ? <PreviewCard embed={embed} /> : null}
				</div>
			)}

			<p className="text-xs text-muted-foreground">
				Rendered from the <strong>saved</strong> message, as a made-up member named NewMember.
			</p>
		</div>
	)
}

/**
 * The Discord embed, drawn the way Discord draws it.
 *
 * A preview that showed only the message text would let a broken image URL or a
 * card with no visible field pass unnoticed until a real member joined, which is
 * the one moment nobody wants to discover it.
 */
function PreviewCard({ embed }: { embed: NonNullable<IWelcomePreviewResponse["embed"]> }) {
	const accent = embed.color === null ? undefined : `#${embed.color.toString(16).padStart(6, "0")}`

	return (
		<div
			className="flex max-w-md flex-col gap-2 rounded-md border-l-4 bg-background/80 p-3"
			style={{ borderLeftColor: accent ?? "var(--border)" }}
		>
			{embed.thumbnail_url ? (
				<img src={embed.thumbnail_url} alt="" className="size-16 self-end rounded" />
			) : null}
			{embed.author ? <span className="text-xs text-muted-foreground">{embed.author}</span> : null}
			{embed.title ? <strong className="text-sm">{embed.title}</strong> : null}
			{embed.description ? (
				<p className="text-sm break-words whitespace-pre-wrap">{embed.description}</p>
			) : null}
			{embed.image_url ? <img src={embed.image_url} alt="" className="w-full rounded" /> : null}
		</div>
	)
}

/** The allow-list, rendered as copy-pasteable chips. */
function PlaceholderHints() {
	return (
		<div className="flex flex-wrap items-center gap-1.5 pt-1">
			<span className="text-xs text-muted-foreground">Available:</span>
			{WELCOME_PLACEHOLDERS.map((placeholder) => (
				<code
					key={placeholder}
					className="rounded border border-border bg-muted/50 px-1.5 py-0.5 text-[11px]"
				>
					{placeholder}
				</code>
			))}
		</div>
	)
}

function ToggleRow({
	id,
	label,
	description,
	checked,
	disabled,
	reason,
	onChange,
}: {
	id: string
	label: string
	description: string
	checked: boolean
	disabled: boolean
	reason: string
	onChange: (checked: boolean) => void
}) {
	return (
		<div
			className="flex items-start justify-between gap-4 rounded-lg border border-border px-4 py-3"
			title={disabled ? reason : undefined}
		>
			<div className="flex flex-col gap-0.5">
				<Label htmlFor={id} className={disabled ? "" : "cursor-pointer"}>
					{label}
				</Label>
				<p className="text-xs text-muted-foreground">{description}</p>
			</div>
			<Switch
				id={id}
				checked={checked}
				disabled={disabled}
				onCheckedChange={onChange}
				className="mt-0.5"
			/>
		</div>
	)
}
