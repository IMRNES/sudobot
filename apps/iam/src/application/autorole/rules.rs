use tracing::{info, warn};
use uuid::Uuid;

use crate::domain::autorole::{
    AutoRole, AutoRoleError, AutoRoleRepository, parse_trigger, validate_delay,
};

/// The guild's automatic role rules.
pub struct ListAutoRolesUseCase<A> {
    auto_roles: A,
}

impl<A: AutoRoleRepository> ListAutoRolesUseCase<A> {
    pub fn new(auto_roles: A) -> Self {
        Self { auto_roles }
    }

    pub async fn execute(&self, guild_id: u64) -> Result<Vec<AutoRole>, AutoRoleError> {
        Ok(self.auto_roles.list_for_guild(guild_id).await?)
    }
}

/// A new rule.
#[derive(Debug, Clone)]
pub struct CreateAutoRoleCommand {
    pub guild_id: u64,
    pub role_id: u64,
    pub trigger: String,
    pub delay_seconds: i32,
    pub applies_to_bots: bool,
}

pub struct CreateAutoRoleUseCase<A> {
    auto_roles: A,
}

impl<A: AutoRoleRepository> CreateAutoRoleUseCase<A> {
    pub fn new(auto_roles: A) -> Self {
        Self { auto_roles }
    }

    pub async fn execute(&self, command: CreateAutoRoleCommand) -> Result<AutoRole, AutoRoleError> {
        let trigger = parse_trigger(&command.trigger)?;
        validate_delay(trigger, command.delay_seconds)?;

        // A role granted twice is a duplicate rule, not two grants: the second
        // would race the first on every join and produce a confusing audit trail.
        let existing = self.auto_roles.list_for_guild(command.guild_id).await?;
        if existing
            .iter()
            .any(|rule| rule.role_id == command.role_id && rule.is_active)
        {
            warn!(
                guild_id = command.guild_id,
                role_id = command.role_id,
                "refused a duplicate auto role"
            );
            return Err(AutoRoleError::RoleAlreadyConfigured(command.role_id));
        }

        let now = chrono::Utc::now();
        let rule = AutoRole {
            id: Uuid::new_v4(),
            guild_id: command.guild_id,
            role_id: command.role_id,
            trigger,
            delay_seconds: command.delay_seconds,
            applies_to_bots: command.applies_to_bots,
            is_active: true,
            created_at: now,
            updated_at: now,
        };

        let created = self.auto_roles.create(rule).await?;

        info!(
            guild_id = command.guild_id,
            role_id = command.role_id,
            trigger = trigger.as_str(),
            "auto role created"
        );

        Ok(created)
    }
}

/// A patch to a rule. `None` leaves a field alone.
#[derive(Debug, Clone)]
pub struct UpdateAutoRoleCommand {
    pub id: Uuid,
    pub trigger: Option<String>,
    pub delay_seconds: Option<i32>,
    pub applies_to_bots: Option<bool>,
    pub is_active: Option<bool>,
}

pub struct UpdateAutoRoleUseCase<A> {
    auto_roles: A,
}

impl<A: AutoRoleRepository> UpdateAutoRoleUseCase<A> {
    pub fn new(auto_roles: A) -> Self {
        Self { auto_roles }
    }

    pub async fn execute(&self, command: UpdateAutoRoleCommand) -> Result<AutoRole, AutoRoleError> {
        // The rule is read first so the delay can be validated against the trigger
        // it will end up with: a patch that only changes `delay_seconds` still has
        // to satisfy the trigger that is already stored.
        let existing = self
            .auto_roles
            .find_by_id(command.id)
            .await?
            .ok_or(AutoRoleError::NotFound(command.id))?;

        let trigger = match &command.trigger {
            Some(raw) => parse_trigger(raw)?,
            None => existing.trigger,
        };

        if let Some(delay_seconds) = command.delay_seconds {
            validate_delay(trigger, delay_seconds)?;
        }

        let updated = self
            .auto_roles
            .update(
                command.id,
                command.trigger.as_deref().map(|_| trigger),
                command.delay_seconds,
                command.applies_to_bots,
                command.is_active,
            )
            .await?;

        info!(
            auto_role_id = %command.id,
            trigger = trigger.as_str(),
            "auto role updated"
        );

        Ok(updated)
    }
}

/// Removes a rule.
pub struct DeleteAutoRoleUseCase<A> {
    auto_roles: A,
}

impl<A: AutoRoleRepository> DeleteAutoRoleUseCase<A> {
    pub fn new(auto_roles: A) -> Self {
        Self { auto_roles }
    }

    /// Read first so deleting something that is already gone is a 404 rather than
    /// a silent success.
    pub async fn execute(&self, id: Uuid) -> Result<(), AutoRoleError> {
        if self.auto_roles.find_by_id(id).await?.is_none() {
            return Err(AutoRoleError::NotFound(id));
        }

        self.auto_roles.delete(id).await?;

        info!(auto_role_id = %id, "auto role deleted");
        Ok(())
    }
}
