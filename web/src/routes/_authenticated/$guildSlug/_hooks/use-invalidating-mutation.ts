import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { normalizeApiError } from "#/libs/errors/index.ts"

/**
 * Mutation wrapper shared by the app's writes.
 *
 * It adds three things over a raw `useMutation`: it invalidates exactly the
 * keys the caller names, it reports outcomes through `sonner` using the
 * server's own message, and it exposes the normalised error so a form can
 * render an inline field error next to the offending input.
 *
 * The invalidation list stays at the call site on purpose — "which lists does
 * editing a shop item change?" is a question only the caller can answer, and a
 * blanket `invalidateQueries()` would refetch the session and the sidebar on
 * every save.
 */
export function useInvalidatingMutation<TData, TVariables>(options: {
	mutationFn: (variables: TVariables) => Promise<TData>
	/** Query keys to invalidate once the write succeeds. */
	invalidateKeys?: readonly (readonly unknown[])[]
	successMessage?: string | ((data: TData, variables: TVariables) => string)
	/** Replaces the generic failure toast title. */
	errorMessage?: string
	/** Runs after invalidation and the success toast; use for closing dialogs. */
	onSuccess?: (data: TData, variables: TVariables) => void
}) {
	const queryClient = useQueryClient()

	return useMutation({
		mutationFn: options.mutationFn,
		onSuccess: (data, variables) => {
			for (const key of options.invalidateKeys ?? []) {
				void queryClient.invalidateQueries({ queryKey: key })
			}
			if (options.successMessage !== undefined) {
				const message =
					typeof options.successMessage === "function"
						? options.successMessage(data, variables)
						: options.successMessage
				toast.success(message)
			}
			options.onSuccess?.(data, variables)
		},
		onError: (error) => {
			const normalised = normalizeApiError(error)
			toast.error(
				normalised.status === 403 ? "Not permitted" : (options.errorMessage ?? "That did not work"),
				{ description: normalised.message },
			)
		},
	})
}
