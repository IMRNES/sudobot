import { useForm } from "@tanstack/react-form"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { FormField, FormRow } from "#/components/features/form-field.tsx"
import { PermissionButton } from "#/components/features/permission-button.tsx"
import { QueryBoundary } from "#/components/features/query-boundary.tsx"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "#/components/ui/card.tsx"
import { Input } from "#/components/ui/input.tsx"
import { Label } from "#/components/ui/label.tsx"
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "#/components/ui/select.tsx"
import { Switch } from "#/components/ui/switch.tsx"
import { levelingApi } from "#/libs/api/resources.ts"
import type { ILevelingConfigResponse, TLevelingConfigUpdateRequest } from "#/libs/api/types.ts"
import { normalizeApiError } from "#/libs/errors/index.ts"
import { queryKeys } from "#/libs/query/keys.ts"
import { formatDuration } from "#/libs/utils.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * Leveling configuration.
 *
 * The form is seeded from the server's `LevelingConfigResponse` and submits a
 * `PATCH` carrying only the fields the admin actually changed — a partial
 * update, matching the route's documented `PATCH` semantics. Switches save
 * immediately (they read as toggles), text and number fields collect into one
 * Save button (they read as a form), which is why the two paths are separate
 * mutations against the same key.
 */

const CURVES = ["mrn", "linear", "flat"] as const

export function LevelingConfigForm({
	guildId,
	canWrite,
	deniedReason,
}: {
	guildId: string
	canWrite: boolean
	deniedReason: string
}) {
	const query = useQuery(guildQueries.levelingConfig(guildId))

	return (
		<Card>
			<CardHeader>
				<CardTitle className="text-base">XP settings</CardTitle>
				<CardDescription>
					How much XP a message grants, how often, and how the level curve is shaped.
				</CardDescription>
			</CardHeader>
			<CardContent>
				<QueryBoundary isLoading={query.isLoading} isError={query.isError} error={query.error}>
					{query.data ? (
						// Remount when the server's version changes, so a refetch never
						// silently discards what the admin is typing.
						<ConfigForm
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

function ConfigForm({
	guildId,
	config,
	canWrite,
	deniedReason,
}: {
	guildId: string
	config: ILevelingConfigResponse
	canWrite: boolean
	deniedReason: string
}) {
	const queryClient = useQueryClient()

	const save = useMutation({
		mutationFn: (body: TLevelingConfigUpdateRequest) => levelingApi.updateConfig(guildId, body),
		onSuccess: (updated) => {
			queryClient.setQueryData(queryKeys.leveling.config(guildId), updated)
			toast.success("Leveling settings saved")
		},
		onError: (error) => {
			const normalised = normalizeApiError(error)
			toast.error("Could not save leveling settings", { description: normalised.message })
		},
	})

	/** Switch flips persist straight away; there is nothing to review. */
	const toggle = (patch: TLevelingConfigUpdateRequest) => {
		if (!canWrite) return
		save.mutate(patch)
	}

	const form = useForm({
		defaultValues: {
			xp_min: String(config.xp_min),
			xp_max: String(config.xp_max),
			cooldown_secs: String(config.cooldown_secs),
			voice_xp_multiplier: String(config.voice_xp_multiplier),
			curve: config.curve,
			announce_channel_id: config.announce_channel_id ?? "",
		},
		onSubmit: ({ value }) => {
			save.mutate({
				xp_min: Number.parseInt(value.xp_min, 10),
				xp_max: Number.parseInt(value.xp_max, 10),
				cooldown_secs: Number.parseInt(value.cooldown_secs, 10),
				voice_xp_multiplier: Number.parseFloat(value.voice_xp_multiplier),
				curve: value.curve,
				// Empty means "no announcement" — sent as an explicit null so the
				// server clears it. A non-empty value is a snowflake string.
				announce_channel_id:
					value.announce_channel_id.trim().length === 0 ? null : value.announce_channel_id.trim(),
			})
		},
	})

	return (
		<div className="flex flex-col gap-6">
			<div className="flex flex-col divide-y divide-border rounded-lg border border-border">
				<ToggleRow
					id="leveling-enabled"
					label="Leveling enabled"
					description="Turn the whole XP system off without losing anyone's progress."
					checked={config.enabled}
					disabled={!canWrite}
					reason={deniedReason}
					onChange={(checked) => toggle({ enabled: checked })}
				/>
				<ToggleRow
					id="leveling-stack-roles"
					label="Keep every rung a member has passed"
					description="Off by default: a member wears only the role for their current level, and the previous one is removed. Turn this on to let titles accumulate instead."
					checked={config.stack_roles}
					disabled={!canWrite}
					reason={deniedReason}
					onChange={(checked) => toggle({ stack_roles: checked })}
				/>
			</div>

			<form
				className="flex flex-col gap-4"
				onSubmit={(event) => {
					event.preventDefault()
					event.stopPropagation()
					if (!canWrite) return
					void form.handleSubmit()
				}}
			>
				<FormRow columns={3}>
					<form.Field
						name="xp_min"
						validators={{
							onChange: ({ value }) => {
								const parsed = Number.parseInt(value, 10)
								return Number.isFinite(parsed) && parsed > 0 ? undefined : "Positive whole number."
							},
						}}
					>
						{(field) => (
							<FormField
								label="Minimum XP per message"
								htmlFor={field.name}
								error={field.state.meta.errors[0]?.toString()}
							>
								<Input
									id={field.name}
									type="number"
									min={1}
									value={field.state.value}
									disabled={!canWrite}
									onBlur={field.handleBlur}
									onChange={(event) => field.handleChange(event.target.value)}
									className="tabular-nums"
								/>
							</FormField>
						)}
					</form.Field>

					<form.Field
						name="xp_max"
						validators={{
							onChange: ({ value, fieldApi }) => {
								const parsed = Number.parseInt(value, 10)
								if (!Number.isFinite(parsed) || parsed <= 0) return "Positive whole number."
								const min = Number.parseInt(fieldApi.form.getFieldValue("xp_min"), 10)
								return parsed < min ? "Must be at least the minimum." : undefined
							},
						}}
					>
						{(field) => (
							<FormField
								label="Maximum XP per message"
								htmlFor={field.name}
								error={field.state.meta.errors[0]?.toString()}
							>
								<Input
									id={field.name}
									type="number"
									min={1}
									value={field.state.value}
									disabled={!canWrite}
									onBlur={field.handleBlur}
									onChange={(event) => field.handleChange(event.target.value)}
									className="tabular-nums"
								/>
							</FormField>
						)}
					</form.Field>

					<form.Field
						name="cooldown_secs"
						validators={{
							onChange: ({ value }) => {
								const parsed = Number.parseInt(value, 10)
								return Number.isFinite(parsed) && parsed >= 0 ? undefined : "Zero or more seconds."
							},
						}}
					>
						{(field) => (
							<FormField
								label="Cooldown"
								htmlFor={field.name}
								description={`Seconds between awards — ${formatDuration(Number.parseInt(field.state.value, 10) || 0)}.`}
								error={field.state.meta.errors[0]?.toString()}
							>
								<Input
									id={field.name}
									type="number"
									min={0}
									value={field.state.value}
									disabled={!canWrite}
									onBlur={field.handleBlur}
									onChange={(event) => field.handleChange(event.target.value)}
									className="tabular-nums"
								/>
							</FormField>
						)}
					</form.Field>
				</FormRow>

				<FormRow>
					<form.Field
						name="voice_xp_multiplier"
						validators={{
							onChange: ({ value }) => {
								const parsed = Number.parseFloat(value)
								return Number.isFinite(parsed) && parsed >= 0 ? undefined : "Zero or more."
							},
						}}
					>
						{(field) => (
							<FormField
								label="Voice XP multiplier"
								htmlFor={field.name}
								description="1.0 grants the same XP in voice as in text."
								error={field.state.meta.errors[0]?.toString()}
							>
								<Input
									id={field.name}
									type="number"
									step="0.1"
									min={0}
									value={field.state.value}
									disabled={!canWrite}
									onBlur={field.handleBlur}
									onChange={(event) => field.handleChange(event.target.value)}
									className="tabular-nums"
								/>
							</FormField>
						)}
					</form.Field>

					<form.Field
						name="announce_channel_id"
						validators={{
							onChange: ({ value }) => {
								const trimmed = value.trim()
								if (trimmed.length === 0) return undefined
								return /^\d{5,25}$/.test(trimmed)
									? undefined
									: "A Discord channel id — digits only."
							},
						}}
					>
						{(field) => (
							<FormField
								label="Level-up channel"
								htmlFor={field.name}
								description="Where the bot announces a level-up. Leave empty for no announcement."
								error={field.state.meta.errors[0]?.toString()}
							>
								<Input
									id={field.name}
									value={field.state.value}
									disabled={!canWrite}
									placeholder="123456789012345678"
									onBlur={field.handleBlur}
									onChange={(event) => field.handleChange(event.target.value)}
									className="font-mono tabular-nums"
								/>
							</FormField>
						)}
					</form.Field>

					<form.Field name="curve">
						{(field) => (
							<FormField
								label="Level curve"
								htmlFor={field.name}
								description="How much XP each successive level costs."
							>
								<Select
									value={field.state.value}
									disabled={!canWrite}
									onValueChange={(value) => field.handleChange(value)}
								>
									<SelectTrigger id={field.name} className="w-full">
										<SelectValue placeholder="Pick a curve" />
									</SelectTrigger>
									<SelectContent>
										{CURVES.map((curve) => (
											<SelectItem key={curve} value={curve}>
												{curve}
											</SelectItem>
										))}
									</SelectContent>
								</Select>
							</FormField>
						)}
					</form.Field>
				</FormRow>

				<div className="flex justify-end">
					<PermissionButton
						type="submit"
						allowed={canWrite}
						reason={deniedReason}
						disabled={save.isPending}
					>
						{save.isPending ? "Saving…" : "Save XP settings"}
					</PermissionButton>
				</div>
			</form>
		</div>
	)
}

/**
 * A labelled switch row. The whole row is the target, and when the member lacks
 * the permission the `Label` still describes the setting — they can read what
 * it does even though they cannot flip it.
 */
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
	const control = (
		<div className="flex items-start justify-between gap-4 px-4 py-3">
			<div className="flex flex-col gap-0.5">
				<Label htmlFor={id} className="cursor-pointer">
					{label}
				</Label>
				<p className="text-xs text-muted-foreground">{description}</p>
			</div>
			<Switch
				id={id}
				checked={checked}
				disabled={disabled}
				title={disabled ? reason : undefined}
				onCheckedChange={onChange}
				className="mt-0.5"
			/>
		</div>
	)

	if (!disabled) return control

	return (
		<div className="cursor-not-allowed opacity-70" title={reason}>
			{control}
		</div>
	)
}
