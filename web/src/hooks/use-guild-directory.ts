import { useQuery } from "@tanstack/react-query"
import { useMemo } from "react"
import type { IDirectoryChannel, IDirectoryRole } from "#/libs/api/types.ts"
import { guildQueries } from "#/routes/_authenticated/$guildSlug/_data/queries.ts"

/**
 * The guild's roles and channels, with names.
 *
 * Every configuration screen stores snowflakes; this is how they get rendered as
 * something a human can read. The lookup is deliberately one hook that returns
 * `Map`s rather than a `nameOf()` per component: six panels on one page then share
 * a single request and a single index, instead of each building its own.
 *
 * Discord is the only source of truth for these names — the API stores no copy —
 * so a role renamed in Discord shows its new name on the next load. `refresh()`
 * exists for the case where an admin renames something and does not want to
 * reload the page.
 */
export function useGuildDirectory(guildId: string) {
	const query = useQuery(guildQueries.directory(guildId))

	const rolesById = useMemo(() => {
		const index = new Map<string, IDirectoryRole>()
		for (const role of query.data?.roles ?? []) index.set(role.id, role)
		return index
	}, [query.data])

	const channelsById = useMemo(() => {
		const index = new Map<string, IDirectoryChannel>()
		for (const channel of query.data?.channels ?? []) index.set(channel.id, channel)
		return index
	}, [query.data])

	return {
		roles: query.data?.roles ?? [],
		channels: query.data?.channels ?? [],
		rolesById,
		channelsById,
		isLoading: query.isLoading,
		/** True when the names could not be read; ids still render. */
		isUnavailable: query.isError,
		refresh: query.refetch,
	}
}

/**
 * A role name, marked with `@`, or `null` when it cannot be resolved.
 *
 * Returning `null` rather than the id is deliberate: callers decide whether to
 * fall back to a raw id or say "unknown". Baking the id in here would make it
 * impossible to tell a resolved name from an unresolved one, which is exactly the
 * distinction that lets a page degrade honestly.
 */
export function roleLabel(rolesById: Map<string, IDirectoryRole>, roleId: string): string | null {
	const role = rolesById.get(roleId)
	return role === undefined ? null : `@${role.name}`
}

/** A channel name, marked with `#`, or `null` when it cannot be resolved. */
export function channelLabel(
	channelsById: Map<string, IDirectoryChannel>,
	channelId: string,
): string | null {
	const channel = channelsById.get(channelId)
	return channel === undefined ? null : `#${channel.name}`
}

/**
 * Channels an admin can sensibly point an announcement at.
 *
 * Voice, category and stage channels are dropped: the bot posts messages, and
 * offering a category as a level-up announcement is offering a channel that can
 * only fail. Forum and announcement channels are kept — the bot can post to both.
 */
export function announceableChannels(channels: IDirectoryChannel[]): IDirectoryChannel[] {
	const ANNOUNCEABLE = new Set([0, 5, 15])
	return channels.filter((channel) => ANNOUNCEABLE.has(channel.kind))
}

/**
 * Roles Discord will let anyone assign by hand.
 *
 * Managed roles belong to a bot or an integration; picking one in a level reward
 * produces a role the bot cannot grant, so the pickers omit them and an admin is
 * never offered a choice that silently does nothing.
 */
export function assignableRoles(roles: IDirectoryRole[]): IDirectoryRole[] {
	return roles.filter((role) => !role.managed)
}
