import { useForm } from "@tanstack/react-form"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import { toast } from "sonner"
import { FormField } from "#/components/features/form-field.tsx"
import { PermissionButton } from "#/components/features/permission-button.tsx"
import { QueryBoundary } from "#/components/features/query-boundary.tsx"
import { StatCard } from "#/components/features/stat-card.tsx"
import { PageHeader } from "#/components/layout/page-header.tsx"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { Input } from "#/components/ui/input.tsx"
import { useCan } from "#/hooks/use-can.ts"
import { useMe } from "#/hooks/use-me.ts"
import { guildsApi } from "#/libs/api/resources.ts"
import type { IGuildDetail, IGuildSummary } from "#/libs/api/types.ts"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { formatDateTime, formatNumber } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * Guild settings. Today that is the display name (`PATCH /guilds/{guildId}`,
 * gated on `dashboard:manage`), plus read-only metadata the API already knows.
 *
 * The name is the one field here that other people notice, so the form reports
 * success explicitly and re-seeds the session cache — the sidebar and the guild
 * switcher both read the name from `/me`, and a stale name there would be
 * immediately visible.
 */
export const Route = createFileRoute("/_authenticated/$guildSlug/settings")({
	component: SettingsPage,
})

function SettingsPage() {
	const { guildSlug } = Route.useParams()
	const { session } = Route.useRouteContext()
	const guild = session?.guild ?? null
	const manage = useCan("dashboard:manage", guild)

	const query = useQuery(guildQueries.detail(guildSlug))

	return (
		<div className="flex flex-col gap-6">
			<PageHeader title="Settings" description="The basics of this server as sudobot sees them." />

			<QueryBoundary isLoading={query.isLoading} isError={query.isError} error={query.error}>
				{query.data ? (
					<>
						<GuildMetadata guild={query.data} viewerRole={guild} />
						<GuildNameForm
							key={query.data.name}
							guild={query.data}
							canManage={manage.allowed}
							deniedReason={manage.reason}
						/>
					</>
				) : null}
			</QueryBoundary>
		</div>
	)
}

/**
 * Read-only facts about the guild.
 *
 * `viewerRole` is separate from `guild` on purpose. The role shown here is *the
 * reader's* role, which is per-person and comes from the session payload — the
 * guild detail endpoint describes the server and has no notion of who is asking.
 * The two used to be one object, and because `/guilds/{id}` does not send
 * `role_slug` the card came back blank rather than wrong, which is exactly the
 * kind of bug that survives review: nothing crashed, one value was simply empty.
 */
function GuildMetadata({
	guild,
	viewerRole,
}: {
	guild: IGuildDetail
	/** The caller's role in this guild, from `/me`; `null` before the session loads. */
	viewerRole: IGuildSummary | null
}) {
	return (
		<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
			<StatCard label="Members" value={formatNumber(guild.member_count)} />
			<StatCard
				label="Your role"
				value={viewerRole?.role_name ?? "—"}
				hint={viewerRole?.role_slug}
			/>
			<StatCard
				label="Server id"
				value={<span className="font-mono text-base">{guild.id}</span>}
				hint="Snowflake"
			/>
			<StatCard
				label="Last updated"
				value={<span className="text-base">{formatDateTime(guild.updated_at)}</span>}
				hint={`Created ${formatDateTime(guild.created_at)}`}
			/>
		</div>
	)
}

function GuildNameForm({
	guild,
	canManage,
	deniedReason,
}: {
	guild: IGuildDetail
	canManage: boolean
	deniedReason: string
}) {
	const queryClient = useQueryClient()
	const me = useMe()

	const save = useMutation({
		mutationFn: (body: { name: string }) => guildsApi.update(guild.id, body),
		onSuccess: (updated) => {
			queryClient.setQueryData(queryKeys.guilds.detail(guild.id), updated)
			// The guild name is echoed in the sidebar and the guild switcher, both of
			// which read the session payload — keep them in step without a full
			// refetch of `/me`.
			queryClient.setQueryData(queryKeys.session.me(), (previous: typeof me.data) =>
				previous === undefined
					? previous
					: {
							...previous,
							guilds: previous.guilds.map((entry) =>
								entry.id === updated.id ? { ...entry, name: updated.name } : entry,
							),
						},
			)
			toast.success("Server name updated")
		},
		onError: (error) => {
			const normalised = normalizeApiError(error)
			toast.error("Could not rename this server", { description: normalised.message })
		},
	})

	const form = useForm({
		defaultValues: { name: guild.name },
		onSubmit: ({ value }) => {
			save.mutate({ name: value.name.trim() })
		},
	})

	return (
		<Card>
			<CardHeader>
				<CardTitle className="text-base">Server name</CardTitle>
				<CardDescription>
					What sudobot calls this server in its dashboard and in messages it posts.
				</CardDescription>
			</CardHeader>
			<CardContent>
				<form
					className="flex max-w-md flex-col gap-4"
					onSubmit={(event) => {
						event.preventDefault()
						event.stopPropagation()
						if (!canManage) return
						void form.handleSubmit()
					}}
				>
					<form.Field
						name="name"
						validators={{
							onChange: ({ value }) => {
								const trimmed = value.trim()
								if (trimmed.length === 0) return "A name is required."
								if (trimmed.length > 100) return "Keep it under 100 characters."
								return undefined
							},
						}}
					>
						{(field) => (
							<FormField
								label="Name"
								htmlFor={field.name}
								required
								error={field.state.meta.errors[0]?.toString()}
							>
								<Input
									id={field.name}
									value={field.state.value}
									disabled={!canManage}
									onBlur={field.handleBlur}
									onChange={(event) => field.handleChange(event.target.value)}
								/>
							</FormField>
						)}
					</form.Field>

					<div className="flex items-center gap-2">
						<PermissionButton
							type="submit"
							allowed={canManage}
							reason={deniedReason}
							disabled={save.isPending}
						>
							{save.isPending ? "Saving…" : "Save name"}
						</PermissionButton>
						<form.Subscribe selector={(state) => state.values.name}>
							{(name) =>
								name !== guild.name ? (
									<button
										type="button"
										className="rounded-sm text-xs text-muted-foreground underline-offset-4 hover:underline"
										onClick={() => form.reset()}
									>
										Discard changes
									</button>
								) : null
							}
						</form.Subscribe>
					</div>
				</form>
			</CardContent>
		</Card>
	)
}
