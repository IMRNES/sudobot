import { api, type IRequestOptions } from "#/libs/api/client.ts"
import type {
	IAccountAdjustRequest,
	IAccountResponse,
	IAutoRoleCreateRequest,
	IAutoRoleResponse,
	IEconomyLeaderboardEntryResponse,
	IGuildDetail,
	IGuildDirectoryResponse,
	IGuildSummary,
	IJailConfigResponse,
	IJailConfigUpdateRequest,
	IJailMemberRequest,
	IJailReleaseResponse,
	IJailSentenceResponse,
	ILeaderboardEntryResponse,
	ILevelingConfigResponse,
	IListParams,
	IMemberResponse,
	IMemberUpdateRequest,
	IPanelCreateRequest,
	IPanelOptionCreateRequest,
	IPanelOptionResponse,
	IPanelPublishRequest,
	IPanelResponse,
	IPanelSummaryResponse,
	IPermissionResponse,
	IRewardCreateRequest,
	IRewardResponse,
	IRoleCreateRequest,
	IRolePermissionsUpdateRequest,
	IRoleResponse,
	IRoleUpdateRequest,
	IShopItemCreateRequest,
	IShopItemResponse,
	ITransactionResponse,
	IWelcomeConfigResponse,
	IWelcomePreviewResponse,
	TAutoRoleUpdateRequest,
	TLevelingConfigUpdateRequest,
	TPanelUpdateRequest,
	TShopItemUpdateRequest,
	TWelcomeConfigUpdateRequest,
} from "#/libs/api/types.ts"

/**
 * One function per documented route. Nothing here knows about React or the
 * query cache, which keeps `_data/` query-option factories trivial and lets any
 * caller (loader, mutation, imperative action) reuse the same call.
 *
 * `signal` is threaded through everywhere so a component that unmounts cancels
 * its in-flight request instead of resolving into a dead cache entry.
 */

type TSignal = Pick<IRequestOptions, "signal">

/* -------------------------------------------------------------------------- */
/* Guilds                                                                      */
/* -------------------------------------------------------------------------- */

export const guildsApi = {
	list: (params: IListParams = {}, options: TSignal = {}) =>
		api.list<IGuildSummary>("/guilds", params, options),

	get: (guildId: string, options: TSignal = {}) =>
		api.get<IGuildDetail>(`/guilds/${guildId}`, options),

	update: (guildId: string, body: { name: string }) =>
		api.patch<IGuildDetail>(`/guilds/${guildId}`, { body }),

	/** Role and channel names, so pickers can show "@Neophyte" not a snowflake. */
	directory: (guildId: string, options: TSignal = {}) =>
		api.get<IGuildDirectoryResponse>(`/guilds/${guildId}/directory`, options),
}

/* -------------------------------------------------------------------------- */
/* Members                                                                     */
/* -------------------------------------------------------------------------- */

export const membersApi = {
	list: (guildId: string, params: IListParams = {}, options: TSignal = {}) =>
		api.list<IMemberResponse>(`/guilds/${guildId}/members`, params, options),

	/** Change the dashboard role held by a member. */
	updateRole: (guildId: string, userId: string, body: IMemberUpdateRequest) =>
		api.patch<IMemberResponse>(`/guilds/${guildId}/members/${userId}`, { body }),

	remove: (guildId: string, userId: string) =>
		api.delete<null>(`/guilds/${guildId}/members/${userId}`),
}

/* -------------------------------------------------------------------------- */
/* Roles and permissions                                                       */
/* -------------------------------------------------------------------------- */

export const rolesApi = {
	list: (guildId: string, params: IListParams = {}, options: TSignal = {}) =>
		api.list<IRoleResponse>(`/guilds/${guildId}/roles`, params, options),

	/** The full catalogue of grantable permissions, for the matrix columns. */
	permissions: (guildId: string, options: TSignal = {}) =>
		api.list<IPermissionResponse>(`/guilds/${guildId}/permissions`, { per_page: 200 }, options),

	create: (guildId: string, body: IRoleCreateRequest) =>
		api.post<IRoleResponse>(`/guilds/${guildId}/roles`, { body }),

	update: (guildId: string, roleId: string, body: IRoleUpdateRequest) =>
		api.patch<IRoleResponse>(`/guilds/${guildId}/roles/${roleId}`, { body }),

	remove: (guildId: string, roleId: string) =>
		api.delete<null>(`/guilds/${guildId}/roles/${roleId}`),

	/** Replaces the role's whole permission set; the response is the new role. */
	setPermissions: (guildId: string, roleId: string, body: IRolePermissionsUpdateRequest) =>
		api.put<IRoleResponse>(`/guilds/${guildId}/roles/${roleId}/permissions`, { body }),
}

/* -------------------------------------------------------------------------- */
/* Economy                                                                     */
/* -------------------------------------------------------------------------- */

export const economyApi = {
	leaderboard: (guildId: string, limit = 10, options: TSignal = {}) =>
		api.list<IEconomyLeaderboardEntryResponse>(
			`/guilds/${guildId}/economy/leaderboard`,
			{},
			{ ...options, query: { limit } },
		),

	account: (guildId: string, discordId: string, options: TSignal = {}) =>
		api.get<IAccountResponse>(`/guilds/${guildId}/economy/accounts/${discordId}`, options),

	transactions: (
		guildId: string,
		discordId: string,
		params: IListParams = {},
		options: TSignal = {},
	) =>
		api.list<ITransactionResponse>(
			`/guilds/${guildId}/economy/accounts/${discordId}/transactions`,
			params,
			options,
		),

	/** Positive `amount` grants, negative deducts. Returns the updated account. */
	adjust: (guildId: string, discordId: string, body: IAccountAdjustRequest) =>
		api.post<IAccountResponse>(`/guilds/${guildId}/economy/accounts/${discordId}/adjust`, { body }),

	items: (guildId: string, params: IListParams = {}, options: TSignal = {}) =>
		api.list<IShopItemResponse>(`/guilds/${guildId}/economy/items`, params, options),

	createItem: (guildId: string, body: IShopItemCreateRequest) =>
		api.post<IShopItemResponse>(`/guilds/${guildId}/economy/items`, { body }),

	updateItem: (guildId: string, itemId: string, body: TShopItemUpdateRequest) =>
		api.patch<IShopItemResponse>(`/guilds/${guildId}/economy/items/${itemId}`, { body }),

	deleteItem: (guildId: string, itemId: string) =>
		api.delete<null>(`/guilds/${guildId}/economy/items/${itemId}`),
}

/* -------------------------------------------------------------------------- */
/* Leveling                                                                    */
/* -------------------------------------------------------------------------- */

export const levelingApi = {
	config: (guildId: string, options: TSignal = {}) =>
		api.get<ILevelingConfigResponse>(`/guilds/${guildId}/leveling/config`, options),

	updateConfig: (guildId: string, body: TLevelingConfigUpdateRequest) =>
		api.patch<ILevelingConfigResponse>(`/guilds/${guildId}/leveling/config`, { body }),

	leaderboard: (guildId: string, limit = 25, options: TSignal = {}) =>
		api.list<ILeaderboardEntryResponse>(
			`/guilds/${guildId}/leveling/leaderboard`,
			{},
			{ ...options, query: { limit } },
		),

	rewards: (guildId: string, params: IListParams = {}, options: TSignal = {}) =>
		api.list<IRewardResponse>(`/guilds/${guildId}/leveling/rewards`, params, options),

	createReward: (guildId: string, body: IRewardCreateRequest) =>
		api.post<IRewardResponse>(`/guilds/${guildId}/leveling/rewards`, { body }),

	deleteReward: (guildId: string, rewardId: string) =>
		api.delete<null>(`/guilds/${guildId}/leveling/rewards/${rewardId}`),
}

/* -------------------------------------------------------------------------- */
/* Welcome                                                                     */
/* -------------------------------------------------------------------------- */

export const welcomeApi = {
	get: (guildId: string, options: TSignal = {}) =>
		api.get<IWelcomeConfigResponse>(`/guilds/${guildId}/welcome`, options),

	update: (guildId: string, body: TWelcomeConfigUpdateRequest) =>
		api.patch<IWelcomeConfigResponse>(`/guilds/${guildId}/welcome`, { body }),

	/**
	 * What a join message would look like right now.
	 *
	 * `rendered` is null when the welcome is disabled or has no message, which is a
	 * normal answer rather than an error — the dashboard shows "nothing would be
	 * sent" instead of an empty preview.
	 */
	preview: (guildId: string, options: TSignal = {}) =>
		api.get<IWelcomePreviewResponse>(`/guilds/${guildId}/welcome/preview`, options),
}

/* -------------------------------------------------------------------------- */
/* Role panels                                                                 */
/* -------------------------------------------------------------------------- */

export const panelsApi = {
	list: (guildId: string, params: IListParams = {}, options: TSignal = {}) =>
		api.list<IPanelSummaryResponse>(`/guilds/${guildId}/panels`, params, options),

	create: (guildId: string, body: IPanelCreateRequest) =>
		api.post<IPanelResponse>(`/guilds/${guildId}/panels`, { body }),

	get: (guildId: string, panelId: string, options: TSignal = {}) =>
		api.get<IPanelResponse>(`/guilds/${guildId}/panels/${panelId}`, options),

	update: (guildId: string, panelId: string, body: TPanelUpdateRequest) =>
		api.patch<IPanelResponse>(`/guilds/${guildId}/panels/${panelId}`, { body }),

	/** A panel holds at most 25 options; the server returns 400 beyond that. */
	createOption: (guildId: string, panelId: string, body: IPanelOptionCreateRequest) =>
		api.post<IPanelOptionResponse>(`/guilds/${guildId}/panels/${panelId}/options`, { body }),

	deleteOption: (guildId: string, panelId: string, optionId: string) =>
		api.delete<null>(`/guilds/${guildId}/panels/${panelId}/options/${optionId}`),

	/**
	 * Records where the bot posted a panel.
	 *
	 * The ids come from the bot, not from this call: the API only stores them so a
	 * later edit targets the same message instead of posting a second one. Sending
	 * an empty body — which this used to do — is a 400, because both ids are
	 * required.
	 */
	publish: (guildId: string, panelId: string, body: IPanelPublishRequest) =>
		api.post<IPanelResponse>(`/guilds/${guildId}/panels/${panelId}/publish`, { body }),
}

/* -------------------------------------------------------------------------- */
/* Jail                                                                        */
/* -------------------------------------------------------------------------- */

export const jailApi = {
	config: (guildId: string, options: TSignal = {}) =>
		api.get<IJailConfigResponse>(`/guilds/${guildId}/jail`, options),

	updateConfig: (guildId: string, body: IJailConfigUpdateRequest) =>
		api.patch<IJailConfigResponse>(`/guilds/${guildId}/jail`, { body }),

	sentences: (guildId: string, options: TSignal = {}) =>
		api.get<IJailSentenceResponse[]>(`/guilds/${guildId}/jail/sentences`, options),

	jail: (guildId: string, body: IJailMemberRequest) =>
		api.post<IJailSentenceResponse>(`/guilds/${guildId}/jail/sentences`, { body }),

	release: (guildId: string, discordUserId: string) =>
		api.delete<IJailReleaseResponse>(`/guilds/${guildId}/jail/sentences/${discordUserId}`),
}

/* -------------------------------------------------------------------------- */
/* Autoroles                                                                   */
/* -------------------------------------------------------------------------- */

export const autorolesApi = {
	list: (guildId: string, params: IListParams = {}, options: TSignal = {}) =>
		api.list<IAutoRoleResponse>(`/guilds/${guildId}/autoroles`, params, options),

	create: (guildId: string, body: IAutoRoleCreateRequest) =>
		api.post<IAutoRoleResponse>(`/guilds/${guildId}/autoroles`, { body }),

	update: (guildId: string, id: string, body: TAutoRoleUpdateRequest) =>
		api.patch<IAutoRoleResponse>(`/guilds/${guildId}/autoroles/${id}`, { body }),

	remove: (guildId: string, id: string) => api.delete<null>(`/guilds/${guildId}/autoroles/${id}`),
}
