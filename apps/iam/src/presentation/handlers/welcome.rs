use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use tracing::instrument;

use crate::application::welcome::{
    AddPanelOptionCommand, AddPanelOptionUseCase, BuildWelcomeMessageQuery,
    BuildWelcomeMessageUseCase, DeletePanelOptionUseCase, GetRolePanelUseCase,
    GetWelcomeConfigUseCase, ListRolePanelsUseCase, PublishRolePanelUseCase,
    UpdateWelcomeConfigCommand, UpdateWelcomeConfigUseCase, UpsertRolePanelCommand,
    UpsertRolePanelUseCase, WelcomeKind,
};
use crate::domain::guild::GuildRepository;
use crate::presentation::dto::welcome::{
    AddPanelOptionRequest, PanelResponse, PanelSummaryResponse, PublishPanelRequest,
    UpdateWelcomeConfigRequest, UpsertPanelRequest, WelcomeConfigResponse,
};
use crate::presentation::error::AppError;
use crate::presentation::extract::ValidatedJson;
use crate::presentation::handlers::{authorize, parse_uuid};
use crate::presentation::middleware::AuthenticatedUser;
use crate::presentation::state::AppState;
use crate::presentation::validation::{guild_id_from_path, parse_snowflake, validate_colour};

/// A placeholder snowflake for the preview's `{mention}`.
///
/// Reads as an obviously sample value in the UI rather than a real account.
const SAMPLE_MEMBER_ID: u64 = 1_000_000_000_000_000_000;

/// The guild's welcome and goodbye configuration.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn get_config(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<WelcomeConfigResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "welcome:read").await?;

    let use_case = GetWelcomeConfigUseCase::new(state.welcome.clone());
    let config = use_case.execute(guild_id).await?;

    Ok(Json(config.into()))
}

/// Changes the welcome configuration.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn update_config(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    ValidatedJson(request): ValidatedJson<UpdateWelcomeConfigRequest>,
) -> Result<Json<WelcomeConfigResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "welcome:write").await?;

    let command = to_update_command(guild_id, request)?;

    let use_case = UpdateWelcomeConfigUseCase::new(state.welcome.clone());
    let config = use_case.execute(command).await?;

    Ok(Json(config.into()))
}

/// Applies a partial update, converting snowflakes and validating colours.
///
/// Split out of `update_config` because the conversion is the interesting part and
/// the handler body should read as a sequence of decisions, not of `map` calls.
pub fn to_update_command(
    guild_id: u64,
    request: UpdateWelcomeConfigRequest,
) -> Result<UpdateWelcomeConfigCommand, AppError> {
    let channel = |value: Option<Option<String>>, field: &'static str| {
        value
            .map(|inner| match inner {
                Some(raw) => parse_snowflake(&raw, field).map(Some),
                None => Ok(None),
            })
            .transpose()
    };

    let colour = |value: Option<Option<i32>>, field: &'static str| {
        value
            .map(|inner| match inner {
                Some(value) => validate_colour(value, field).map(Some),
                None => Ok(None),
            })
            .transpose()
    };

    Ok(UpdateWelcomeConfigCommand {
        guild_id,
        welcome_enabled: request.welcome_enabled,
        welcome_channel_id: channel(request.welcome_channel_id, "welcome_channel_id")?,
        welcome_message: request.welcome_message,
        welcome_embed_color: colour(request.welcome_embed_color, "welcome_embed_color")?,
        welcome_dm_enabled: request.welcome_dm_enabled,
        embed_title: request.embed_title,
        embed_description: request.embed_description,
        embed_author: request.embed_author,
        embed_image_url: request.embed_image_url,
        embed_thumbnail_url: request.embed_thumbnail_url,
        goodbye_enabled: request.goodbye_enabled,
        goodbye_channel_id: channel(request.goodbye_channel_id, "goodbye_channel_id")?,
        goodbye_message: request.goodbye_message,
        goodbye_embed_color: colour(request.goodbye_embed_color, "goodbye_embed_color")?,
    })
}

/// The guild's role panels.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn list_panels(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<Vec<PanelSummaryResponse>>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "autorole:read").await?;

    let use_case = ListRolePanelsUseCase::new(state.welcome.clone());
    let panels = use_case.execute(guild_id).await?;

    Ok(Json(
        panels
            .into_iter()
            .map(|(panel, count)| PanelSummaryResponse::new(panel, count))
            .collect(),
    ))
}

/// One panel, with its buttons.
///
/// Addressed by **id**, because that is what `/{panel_id}` carries for every other
/// method on the same path — PATCH, the option routes and publish all parse a UUID.
/// Reading the segment as a slug here made `GET` the one verb that 404'd on a panel
/// the other four could address.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, panel = %panel_id))]
pub async fn get_panel(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, panel_id)): Path<(String, String)>,
) -> Result<Json<PanelResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "autorole:read").await?;

    let panel_id = parse_uuid(&panel_id, "panelId")?;

    let use_case = GetRolePanelUseCase::new(state.welcome.clone());
    // The use case scopes by guild, so a panel id belonging to another guild is a
    // 404 rather than a leak.
    let panel = use_case.execute_by_id(guild_id, panel_id).await?;

    Ok(Json(panel.into()))
}

/// Creates or replaces a panel's header.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn upsert_panel(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    ValidatedJson(request): ValidatedJson<UpsertPanelRequest>,
) -> Result<Json<PanelResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "autorole:write").await?;

    let channel_id = match request.channel_id.as_deref() {
        Some(raw) => Some(parse_snowflake(raw, "channel_id")?),
        None => None,
    };

    let use_case = UpsertRolePanelUseCase::new(state.welcome.clone());
    let panel = use_case
        .execute(UpsertRolePanelCommand {
            guild_id,
            slug: request.slug,
            title: request.title,
            description: request.description,
            channel_id,
        })
        .await?;

    Ok(Json(panel.into()))
}

/// Rewrites a panel's header.
///
/// Separate from `upsert_panel` only in how it is addressed: that one takes a slug
/// from the body, this one takes the id from the path. Both end in the same use
/// case, which is what keeps "create" and "rename" from drifting apart.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, panel = %panel_id))]
pub async fn update_panel(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, panel_id)): Path<(String, String)>,
    ValidatedJson(request): ValidatedJson<UpsertPanelRequest>,
) -> Result<Json<PanelResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let panel_id = parse_uuid(&panel_id, "panelId")?;
    authorize(&state, actor.id, guild_id, "autorole:write").await?;

    let channel_id = match request.channel_id.as_deref() {
        Some(raw) => Some(parse_snowflake(raw, "channel_id")?),
        None => None,
    };

    let use_case = UpsertRolePanelUseCase::new(state.welcome.clone());
    use_case
        .execute(UpsertRolePanelCommand {
            guild_id,
            slug: request.slug,
            title: request.title,
            description: request.description,
            channel_id,
        })
        .await?;

    // Re-read through the id so a slug rename cannot change which row the response
    // describes.
    let detail = GetRolePanelUseCase::new(state.welcome.clone());
    Ok(Json(detail.execute_by_id(guild_id, panel_id).await?.into()))
}

/// Adds a button to a panel.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, panel = %panel_id))]
pub async fn add_option(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, panel_id)): Path<(String, String)>,
    ValidatedJson(request): ValidatedJson<AddPanelOptionRequest>,
) -> Result<(StatusCode, Json<PanelResponse>), AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let panel_id = parse_uuid(&panel_id, "panelId")?;
    authorize(&state, actor.id, guild_id, "autorole:write").await?;

    let use_case = AddPanelOptionUseCase::new(state.welcome.clone());
    use_case
        .execute(AddPanelOptionCommand {
            guild_id,
            panel_id,
            label: request.label,
            emoji: request.emoji,
            role_id: parse_snowflake(&request.role_id, "role_id")?,
            style: request.style.unwrap_or(2),
            behavior: request.behavior.unwrap_or_else(|| "toggle".to_string()),
        })
        .await?;

    // Re-read so the response carries the whole panel: the client renders buttons
    // from this list and a partial response would make it refetch anyway.
    let detail = GetRolePanelUseCase::new(state.welcome.clone());
    let panel = detail.execute_by_id(guild_id, panel_id).await?;

    Ok((StatusCode::CREATED, Json(panel.into())))
}

/// Removes a button from a panel.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, panel = %panel_id))]
pub async fn delete_option(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, panel_id, option_id)): Path<(String, String, String)>,
) -> Result<StatusCode, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let panel_id = parse_uuid(&panel_id, "panelId")?;
    let option_id = parse_uuid(&option_id, "optionId")?;
    authorize(&state, actor.id, guild_id, "autorole:write").await?;

    let use_case = DeletePanelOptionUseCase::new(state.welcome.clone());
    use_case.execute(guild_id, panel_id, option_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Posts a panel to a Discord channel and records where it landed.
///
/// The message id is not a request field: the API posts with the bot token and gets
/// one back. It used to require the caller to supply both ids, which meant the
/// dashboard could only ever record a message somebody else had posted — so no
/// panel was ever actually posted anywhere.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, panel = %panel_id))]
pub async fn publish_panel(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, panel_id)): Path<(String, String)>,
    ValidatedJson(request): ValidatedJson<PublishPanelRequest>,
) -> Result<Json<PanelResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let panel_id = parse_uuid(&panel_id, "panelId")?;
    authorize(&state, actor.id, guild_id, "autorole:write").await?;

    let use_case =
        PublishRolePanelUseCase::new(state.welcome.clone(), state.panel_publisher.clone());
    let panel = use_case
        .execute(
            guild_id,
            panel_id,
            parse_snowflake(&request.channel_id, "channel_id")?,
        )
        .await?;

    Ok(Json(panel.into()))
}

/// The message the bot would post for a join, for the dashboard's preview.
///
/// Exposed so an admin can see the rendered result rather than a template with
/// braces in it — the quickest way to catch `{membercount}` before members do.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn preview_welcome(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "welcome:read").await?;

    let guild = state
        .guilds
        .find_by_id(guild_id)
        .await?
        .ok_or_else(|| AppError::NotFound("guild not found".into()))?;

    // A sample member, not the caller: the point of the preview is to show what a
    // *new* member would see, and using the admin's own id would put their mention
    // in the rendered text.
    let use_case = BuildWelcomeMessageUseCase::new(state.welcome.clone());
    let message = use_case
        .execute(BuildWelcomeMessageQuery {
            guild_id,
            guild_name: guild.name,
            member_count: i64::from(guild.member_count),
            discord_user_id: SAMPLE_MEMBER_ID,
            username: "NewMember".to_string(),
            kind: WelcomeKind::Welcome,
        })
        .await?;

    Ok(Json(serde_json::json!({
        "rendered": message.map(|message| message.content),
    })))
}
