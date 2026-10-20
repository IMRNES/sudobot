import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Clock, Lock, Unlock } from "lucide-react"
import { useEffect, useState } from "react"
import { toast } from "sonner"
import {
	ChannelPicker,
	DirectoryUnavailableNotice,
	RoleName,
	RolePicker,
} from "#/components/features/discord-pickers.tsx"
import { EmptyState } from "#/components/features/empty-state.tsx"
import { FormField, FormRow } from "#/components/features/form-field.tsx"
import { PermissionButton } from "#/components/features/permission-button.tsx"
import { FormError, QueryBoundary } from "#/components/features/query-boundary.tsx"
import { UserCell } from "#/components/features/user-cell.tsx"
import { Badge } from "#/components/ui/badge.tsx"
import { Card, CardContent, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { Input } from "#/components/ui/input.tsx"
import { useGuildDirectory } from "#/hooks/use-guild-directory.ts"
import { jailApi } from "#/libs/api/resources.ts"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * Renders a sentence's remaining time in the unit a human would say it in.
 *
 * `remaining_seconds` comes from the server rather than being computed from
 * `expires_at` here, because the operator's clock is not the bot's: a machine a few
 * minutes fast would show a prisoner as already free while the sweeper still has
 * them locked up.
 */
function remaining(seconds: number | null): string {
	if (seconds === null) return "indefinite"
	if (seconds <= 0) return "served"
	const minutes = Math.floor(seconds / 60)
	if (minutes >= 60) return `${Math.floor(minutes / 60)}h ${minutes % 60}m`
	if (minutes >= 1) return `${minutes}m`
	return `${seconds}s`
}

/**
 * Hours in the form, seconds on the wire.
 *
 * Empty is `null`, which the API reads as "no default" — not zero, which it rejects.
 */
function hoursToSeconds(hours: string): number | null {
	const parsed = Number(hours)
	if (!hours.trim() || !Number.isFinite(parsed) || parsed <= 0) return null
	return Math.round(parsed * 3600)
}

/**
 * The jail: where prisoners sit, what it costs to leave, and the form that seals the
 * server.
 *
 * The escape routes are the interesting part, so the page states them rather than
 * leaving a moderator to infer them from a role id: time served, a moderator, or
 * bail. Quitting and rejoining is deliberately not among them, and the copy says so —
 * a jail an admin believes is escapable is worse than one they know is not.
 */
export function JailPanel({ guildId, canWrite }: { guildId: string; canWrite: boolean }) {
	const queryClient = useQueryClient()
	const configQuery = useQuery(guildQueries.jailConfig(guildId))
	const sentencesQuery = useQuery(guildQueries.jailSentences(guildId))
	const {
		roles,
		channels,
		rolesById,
		channelsById,
		isLoading: namesLoading,
		isUnavailable,
	} = useGuildDirectory(guildId)

	const config = configQuery.data

	// The pickers speak in strings and treat "" as "nothing selected", which is also
	// what the API sends as `null`; keeping one representation avoids a null/empty
	// mismatch at every call site.
	const [roleId, setRoleId] = useState("")
	const [channelId, setChannelId] = useState("")
	const [defaultHours, setDefaultHours] = useState("")
	const [bailAmount, setBailAmount] = useState("")
	const [dirty, setDirty] = useState(false)

	// Seed the form once the config arrives, and only while it is untouched: a
	// background refetch must not overwrite what the admin is in the middle of typing.
	useEffect(() => {
		if (!config || dirty) return
		setRoleId(config.jail_role_id ?? "")
		setChannelId(config.jail_channel_id ?? "")
		setDefaultHours(
			config.default_sentence_seconds
				? String(Math.round(config.default_sentence_seconds / 3600))
				: "",
		)
		setBailAmount(config.bail_amount ? String(config.bail_amount) : "")
	}, [config, dirty])

	const saveMutation = useMutation({
		mutationFn: () =>
			jailApi.updateConfig(guildId, {
				// "" becomes null, which is the API's "clear it". Sending undefined would
				// mean "leave it alone" and the jail would never switch off.
				jail_role_id: roleId || null,
				jail_channel_id: channelId || null,
				default_sentence_seconds: hoursToSeconds(defaultHours),
				bail_amount: bailAmount.trim() ? Number(bailAmount) : null,
			}),
		onSuccess: () => {
			void queryClient.invalidateQueries({ queryKey: queryKeys.jail.all(guildId) })
			setDirty(false)
			toast.success("Jail saved and applied to Discord")
		},
	})

	const releaseMutation = useMutation({
		mutationFn: (discordUserId: string) => jailApi.release(guildId, discordUserId),
		onSuccess: (result) => {
			void queryClient.invalidateQueries({ queryKey: queryKeys.jail.sentences(guildId) })
			// The two outcomes are genuinely different and the moderator should know
			// which happened: "already free" means they acted on a stale row.
			toast.success(result.was_imprisoned ? "Released" : "They were already free")
		},
	})

	const sentences = sentencesQuery.data ?? []
	const saveError = saveMutation.error ? normalizeApiError(saveMutation.error) : null

	return (
		<div className="flex flex-col gap-6">
			{isUnavailable && <DirectoryUnavailableNotice />}

			<Card>
				<CardHeader>
					<CardTitle className="flex items-center gap-2">
						<Lock className="size-4" />
						Jail configuration
					</CardTitle>
				</CardHeader>
				<CardContent className="flex flex-col gap-4">
					<p className="text-muted-foreground text-sm">
						The role is denied every channel except the one named below. Saving writes a permission
						overwrite into every channel in the server, categories included — a channel created
						later is sealed on the next save.
					</p>

					<QueryBoundary
						isLoading={configQuery.isLoading}
						isError={configQuery.isError}
						error={configQuery.error}
					>
						<div className="flex flex-col gap-4">
							<FormRow>
								<FormField
									label="Prisoner role"
									description="The role a jailed member wears. Clear it to disable the jail."
								>
									<RolePicker
										value={roleId}
										onChange={(next) => {
											setDirty(true)
											setRoleId(next)
										}}
										roles={roles}
										isLoading={namesLoading}
										placeholder="No role — jail disabled"
									/>
								</FormField>

								<FormField
									label="Prison channel"
									description="The only channel a prisoner can still read and write."
								>
									<ChannelPicker
										value={channelId}
										onChange={(next) => {
											setDirty(true)
											setChannelId(next)
										}}
										channels={channels}
										isLoading={namesLoading}
										allowNone
										noneLabel="No channel selected"
									/>
								</FormField>
							</FormRow>

							<FormRow>
								<FormField
									label="Default sentence (hours)"
									description="Used when a moderator does not name a duration. Empty means a duration must always be given."
								>
									<Input
										id="jail-default-hours"
										type="number"
										min="1"
										value={defaultHours}
										onChange={(event) => {
											setDirty(true)
											setDefaultHours(event.target.value)
										}}
										placeholder="e.g. 24"
									/>
								</FormField>

								<FormField
									label="Bail price"
									description="What a prisoner may pay to leave early. Empty disables bail."
								>
									<Input
										id="jail-bail"
										type="number"
										min="1"
										value={bailAmount}
										onChange={(event) => {
											setDirty(true)
											setBailAmount(event.target.value)
										}}
										placeholder="e.g. 500"
									/>
								</FormField>
							</FormRow>

							{saveError && <FormError error={saveError} />}

							<div className="flex items-center gap-3">
								<PermissionButton
									allowed={canWrite}
									reason="You need the members:write permission to change the jail."
									onClick={() => saveMutation.mutate()}
									disabled={saveMutation.isPending}
								>
									{saveMutation.isPending ? "Applying…" : "Save and apply"}
								</PermissionButton>
								{dirty && (
									<span className="text-muted-foreground text-xs">
										Unsaved — nothing changes in Discord until you save.
									</span>
								)}
							</div>
						</div>
					</QueryBoundary>
				</CardContent>
			</Card>

			<Card>
				<CardHeader>
					<CardTitle className="flex items-center gap-2">
						<Unlock className="size-4" />
						Ways out
					</CardTitle>
				</CardHeader>
				<CardContent className="text-muted-foreground flex flex-col gap-2 text-sm">
					<p>
						<strong className="text-foreground">Time served.</strong> A sweep every minute releases
						anyone whose sentence has run out, so a restart does not extend it.
					</p>
					<p>
						<strong className="text-foreground">A moderator.</strong> Releasing here or with{" "}
						<code>/jail release</code> removes the role immediately.
					</p>
					<p>
						<strong className="text-foreground">Bail.</strong> When a price is set, a prisoner can
						run <code>/jail bail</code> to pay it and leave. The charge is a purchase, so it cannot
						overdraw their account.
					</p>
					<p>
						<strong className="text-foreground">Leaving is not a way out.</strong> A member who
						quits and rejoins is put back in the cell on arrival, because the sentence is the row,
						not the role.
					</p>
				</CardContent>
			</Card>

			<Card>
				<CardHeader>
					<CardTitle className="flex items-center gap-2">
						<Clock className="size-4" />
						Current sentences
					</CardTitle>
				</CardHeader>
				<CardContent>
					<QueryBoundary
						isLoading={sentencesQuery.isLoading}
						isError={sentencesQuery.isError}
						error={sentencesQuery.error}
					>
						{sentences.length === 0 ? (
							<EmptyState
								icon={Unlock}
								title="The jail is empty"
								description="Nobody is serving a sentence right now."
							/>
						) : (
							<ul className="flex flex-col gap-3">
								{sentences.map((sentence) => (
									<li
										key={sentence.discord_user_id}
										className="flex flex-wrap items-center justify-between gap-3 rounded-md border p-3"
									>
										<div className="flex min-w-0 flex-col gap-1">
											<UserCell discordId={sentence.discord_user_id} />
											<span className="text-muted-foreground truncate text-xs">
												{sentence.reason || "No reason given"}
											</span>
										</div>
										<div className="flex items-center gap-3">
											<Badge variant="secondary">{remaining(sentence.remaining_seconds)}</Badge>
											<PermissionButton
												allowed={canWrite}
												reason="You need the members:write permission to release members."
												variant="outline"
												size="sm"
												onClick={() => releaseMutation.mutate(sentence.discord_user_id)}
												disabled={releaseMutation.isPending}
											>
												Release
											</PermissionButton>
										</div>
									</li>
								))}
							</ul>
						)}
					</QueryBoundary>
				</CardContent>
			</Card>

			{/* A one-line summary in names rather than snowflakes, so an admin can read
			    the current state without decoding two ids. */}
			{roleId && (
				<p className="text-muted-foreground text-xs">
					Prisoners wear <RoleName roleId={roleId} rolesById={rolesById} isLoading={namesLoading} />
					{channelId && " and can only see "}
					{channelId && <code>{channelsById.get(channelId)?.name ?? `#${channelId}`}</code>}.
				</p>
			)}
		</div>
	)
}
