//! SeaORM-backed implementations of the domain's repository traits.
//!
//! Each file here implements exactly one trait from `domain/*/repository.rs`, and
//! each one owns its `From<Model> for DomainEntity` conversions — the domain must
//! never learn what a SeaORM model looks like.

mod auto_role;
mod economy;
pub mod entities;
mod error;
mod guild;
mod guild_member;
mod guild_role;
mod leveling;
mod mapping;
mod member_directory;
mod pagination;
mod permission;
mod refresh_token;
mod user;
mod welcome;

pub use auto_role::SeaOrmAutoRoleRepository;
pub use economy::SeaOrmEconomyRepository;
pub use error::map_db_err;
pub use guild::SeaOrmGuildRepository;
pub use guild_member::SeaOrmGuildMemberRepository;
pub use guild_role::SeaOrmGuildRoleRepository;
pub use leveling::SeaOrmLevelingRepository;
pub use permission::SeaOrmPermissionRepository;
pub use refresh_token::SeaOrmRefreshTokenRepository;
pub use user::SeaOrmUserRepository;
pub use welcome::SeaOrmWelcomeRepository;
