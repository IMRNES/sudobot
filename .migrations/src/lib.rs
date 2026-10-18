//! Database schema for sudobot.
//!
//! Migrations live here rather than in the API crate so the deploy step can run
//! them without booting the server: `migrate up` is its own binary, and the
//! gateway never applies schema changes as a side effect of starting.

pub use sea_orm_migration::prelude::*;

mod m20261018_000001_create_users;
mod m20261018_000002_create_guilds;
mod m20261018_000003_create_rbac;
mod m20261018_000004_create_guild_members;
mod m20261018_000005_create_economy;
mod m20261018_000006_create_leveling;
mod m20261018_000007_create_welcome;
mod m20261018_000008_create_autoroles;

/// Every migration, in the order they must be applied.
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261018_000001_create_users::Migration),
            Box::new(m20261018_000002_create_guilds::Migration),
            Box::new(m20261018_000003_create_rbac::Migration),
            // After rbac: `guild_members` holds a foreign key to `guild_roles`.
            Box::new(m20261018_000004_create_guild_members::Migration),
            Box::new(m20261018_000005_create_economy::Migration),
            Box::new(m20261018_000006_create_leveling::Migration),
            Box::new(m20261018_000007_create_welcome::Migration),
            Box::new(m20261018_000008_create_autoroles::Migration),
        ]
    }
}

/// Column-name constants shared between migrations and the ORM entities.
///
/// One spelling per column, so a rename is a compile error in the entity instead
/// of a runtime "column does not exist".
pub mod columns {
    pub const ID: &str = "id";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
}
