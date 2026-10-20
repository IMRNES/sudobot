import { useForm } from "@tanstack/react-form"
import { RolePicker } from "#/components/features/discord-pickers.tsx"
import { FormField, FormRow } from "#/components/features/form-field.tsx"
import { FormError } from "#/components/features/query-boundary.tsx"
import { Button } from "#/components/ui/button.tsx"
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "#/components/ui/dialog.tsx"
import { Input } from "#/components/ui/input.tsx"
import { Textarea } from "#/components/ui/textarea.tsx"
import type { IDirectoryRole, IShopItemCreateRequest, IShopItemResponse } from "#/libs/api/types.ts"

/**
 * Create / edit dialog for a shop item.
 *
 * One component for both modes — the only differences are the seed values, the
 * writable fields and the submit handler, and duplicating that structure into
 * two files is how the two drift apart.
 *
 * The body is a separate component keyed by the item id. That is what makes
 * "edit item B after item A" show B's values: React unmounts the old form and
 * mounts a fresh one with the new defaults, instead of TanStack Form keeping
 * A's state alive behind a changed prop.
 */

export interface IItemFormValues {
	slug: string
	name: string
	description: string
	price: string
	role_id: string
	/** Empty string means "unlimited", which the API represents as `null`. */
	stock: string
}

const EMPTY_ITEM: IItemFormValues = {
	slug: "",
	name: "",
	description: "",
	price: "",
	role_id: "",
	stock: "",
}

export function toItemFormValues(item: IShopItemResponse): IItemFormValues {
	return {
		slug: item.slug,
		name: item.name,
		description: item.description ?? "",
		price: String(item.price),
		role_id: item.role_id ?? "",
		stock: item.stock === null ? "" : String(item.stock),
	}
}

/** Convert validated form values into the wire shape documented in API.md. */
export function toShopItemRequest(values: IItemFormValues): IShopItemCreateRequest {
	return {
		slug: values.slug.trim(),
		name: values.name.trim(),
		description: values.description.trim().length > 0 ? values.description.trim() : null,
		price: Number.parseInt(values.price, 10),
		role_id: values.role_id.trim().length > 0 ? values.role_id.trim() : null,
		stock: values.stock.trim().length > 0 ? Number.parseInt(values.stock, 10) : null,
	}
}

export function ItemDialog({
	open,
	onOpenChange,
	mode,
	item,
	onSubmit,
	isPending,
	submitError,
	roles,
}: {
	open: boolean
	onOpenChange: (open: boolean) => void
	mode: "create" | "edit"
	item?: IShopItemResponse
	onSubmit: (payload: IShopItemCreateRequest) => void
	isPending: boolean
	submitError: unknown
	roles: IDirectoryRole[]
}) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-lg">
				<DialogHeader>
					<DialogTitle>{mode === "create" ? "New shop item" : "Edit shop item"}</DialogTitle>
					<DialogDescription>
						{mode === "create"
							? "Members buy this with their balance. Leave the role blank for a cosmetic item."
							: "Changes take effect the next time a member opens the shop."}
					</DialogDescription>
				</DialogHeader>
				{/* Remounting on id is what re-seeds the form; see the file header. */}
				<ItemForm
					key={item?.id ?? "new"}
					mode={mode}
					item={item}
					onSubmit={onSubmit}
					onCancel={() => onOpenChange(false)}
					isPending={isPending}
					submitError={submitError}
					roles={roles}
				/>
			</DialogContent>
		</Dialog>
	)
}

function ItemForm({
	mode,
	item,
	onSubmit,
	onCancel,
	isPending,
	submitError,
	roles,
}: {
	mode: "create" | "edit"
	item?: IShopItemResponse
	onSubmit: (payload: IShopItemCreateRequest) => void
	onCancel: () => void
	isPending: boolean
	submitError: unknown
	roles: IDirectoryRole[]
}) {
	const form = useForm({
		defaultValues: item ? toItemFormValues(item) : EMPTY_ITEM,
		onSubmit: ({ value }) => {
			onSubmit(toShopItemRequest(value))
		},
	})

	return (
		<form
			className="flex flex-col gap-4"
			onSubmit={(event) => {
				event.preventDefault()
				event.stopPropagation()
				void form.handleSubmit()
			}}
		>
			<FormRow>
				<form.Field
					name="name"
					validators={{
						onChange: ({ value }) => (value.trim().length === 0 ? "Required." : undefined),
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
								onBlur={field.handleBlur}
								onChange={(event) => field.handleChange(event.target.value)}
								placeholder="VIP"
							/>
						</FormField>
					)}
				</form.Field>

				<form.Field
					name="slug"
					validators={{
						onChange: ({ value }) =>
							/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value.trim())
								? undefined
								: "Lowercase letters, numbers and hyphens.",
					}}
				>
					{(field) => (
						<FormField
							label="Slug"
							htmlFor={field.name}
							required
							description="Stable identifier used by bot commands."
							error={field.state.meta.errors[0]?.toString()}
						>
							<Input
								id={field.name}
								value={field.state.value}
								onBlur={field.handleBlur}
								onChange={(event) => field.handleChange(event.target.value)}
								placeholder="vip"
								// The API has no route to rename an item's slug after creation.
								disabled={mode === "edit"}
								className="font-mono"
							/>
						</FormField>
					)}
				</form.Field>
			</FormRow>

			<form.Field name="description">
				{(field) => (
					<FormField label="Description" htmlFor={field.name}>
						<Textarea
							id={field.name}
							value={field.state.value}
							onBlur={field.handleBlur}
							onChange={(event) => field.handleChange(event.target.value)}
							placeholder="Grants the VIP role and a coloured name."
							rows={2}
						/>
					</FormField>
				)}
			</form.Field>

			<FormRow columns={3}>
				<form.Field
					name="price"
					validators={{
						onChange: ({ value }) => {
							const parsed = Number.parseInt(value, 10)
							return Number.isFinite(parsed) && parsed >= 0 ? undefined : "Zero or more."
						},
					}}
				>
					{(field) => (
						<FormField
							label="Price"
							htmlFor={field.name}
							required
							error={field.state.meta.errors[0]?.toString()}
						>
							<Input
								id={field.name}
								type="number"
								min={0}
								value={field.state.value}
								onBlur={field.handleBlur}
								onChange={(event) => field.handleChange(event.target.value)}
								placeholder="5000"
								className="tabular-nums"
							/>
						</FormField>
					)}
				</form.Field>

				<form.Field name="stock">
					{(field) => (
						<FormField label="Stock" htmlFor={field.name} description="Blank means unlimited.">
							<Input
								id={field.name}
								type="number"
								min={0}
								value={field.state.value}
								onBlur={field.handleBlur}
								onChange={(event) => field.handleChange(event.target.value)}
								placeholder="∞"
								className="tabular-nums"
							/>
						</FormField>
					)}
				</form.Field>

				<form.Field
					name="role_id"
					validators={{
						onChange: ({ value }) =>
							value.trim().length === 0 || /^\d{17,20}$/.test(value.trim())
								? undefined
								: "17 to 20 digits.",
					}}
				>
					{(field) => (
						<FormField
							label="Role"
							htmlFor={field.name}
							description="Granted on purchase. Leave blank for a cosmetic item."
							error={field.state.meta.errors[0]?.toString()}
						>
							<RolePicker
								value={field.state.value}
								onChange={field.handleChange}
								roles={roles}
								placeholder="No role"
							/>
						</FormField>
					)}
				</form.Field>
			</FormRow>

			<FormError error={submitError} />

			<DialogFooter>
				<Button type="button" variant="outline" onClick={onCancel}>
					Cancel
				</Button>
				<Button type="submit" disabled={isPending}>
					{isPending ? "Saving…" : mode === "create" ? "Create item" : "Save changes"}
				</Button>
			</DialogFooter>
		</form>
	)
}
