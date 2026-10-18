//! SeaORM entities, one module per table in `.migrations`.
//!
//! Column names and types mirror the migrations exactly; the migrations are the
//! source of truth. Timestamps are `DateTimeWithTimeZone` (`TIMESTAMPTZ`) and are
//! converted to `DateTime<Utc>` when a model becomes a domain entity.

pub mod auto_role_pending;
pub mod auto_roles;
pub mod economy_accounts;
pub mod economy_purchases;
pub mod economy_shop_items;
pub mod economy_transactions;
pub mod guild_member_profiles;
pub mod guild_members;
pub mod guild_role_permissions;
pub mod guild_roles;
pub mod guilds;
pub mod leveling_configs;
pub mod leveling_profiles;
pub mod leveling_rewards;
pub mod permissions;
pub mod refresh_tokens;
pub mod role_panel_options;
pub mod role_panels;
pub mod users;
pub mod welcome_configs;
