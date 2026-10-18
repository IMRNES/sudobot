use tracing::{info, warn};
use uuid::Uuid;

use super::errors::UserAdminError;
use crate::domain::errors::{Page, PaginationParams};
use crate::domain::user::{User, UserError, UserRepository};

/// Every account on the platform.
///
/// A platform-admin view: no guild scoping applies, and the handler is the one
/// that decides who may call it.
pub struct ListUsersUseCase<U> {
    users: U,
}

impl<U: UserRepository> ListUsersUseCase<U> {
    pub fn new(users: U) -> Self {
        Self { users }
    }

    pub async fn execute(&self, params: &PaginationParams) -> Result<Page<User>, UserError> {
        Ok(self.users.list(params).await?)
    }
}

/// One account.
pub struct GetUserUseCase<U> {
    users: U,
}

impl<U: UserRepository> GetUserUseCase<U> {
    pub fn new(users: U) -> Self {
        Self { users }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<User, UserError> {
        self.users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| UserError::not_found_for(user_id))
    }
}

/// Deletes an account, which cascades to its memberships and sessions.
pub struct DeleteUserUseCase<U> {
    users: U,
}

impl<U: UserRepository> DeleteUserUseCase<U> {
    pub fn new(users: U) -> Self {
        Self { users }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<(), UserError> {
        // Reading first is what turns a silent no-op (call it twice, get 204 both
        // times) into an honest 404.
        if self.users.find_by_id(user_id).await?.is_none() {
            return Err(UserError::not_found_for(user_id));
        }

        self.users.delete(user_id).await?;
        info!(user_id = %user_id, "account deleted");
        Ok(())
    }
}

/// Grants or revokes platform-wide authority.
pub struct SetSuperAdminUseCase<U> {
    users: U,
}

impl<U: UserRepository> SetSuperAdminUseCase<U> {
    pub fn new(users: U) -> Self {
        Self { users }
    }

    /// Promotes or demotes an account.
    ///
    /// Refuses to demote the last super admin: without one, nobody can approve
    /// anything platform-wide, and the installation cannot be recovered from the
    /// dashboard at all. The count comes from `UserRepository::list` because the
    /// trait exposes no `count_super_admins`, so the rows are walked rather than
    /// counted — accurate, at the cost of reading the page it needs.
    pub async fn execute(
        &self,
        user_id: Uuid,
        is_super_admin: bool,
    ) -> Result<User, UserAdminError> {
        let user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| UserError::not_found_for(user_id))?;

        // Only a demotion can empty the operator set, and only when the account is
        // actually an operator today — promoting an admin, or demoting somebody who
        // is not one, never needs the count.
        if !is_super_admin && user.is_super_admin && self.is_last_super_admin(user_id).await? {
            warn!(user_id = %user_id, "refused to demote the last super admin");
            return Err(UserAdminError::LastSuperAdmin);
        }

        let updated = self.users.set_super_admin(user_id, is_super_admin).await?;

        info!(
            user_id = %updated.id,
            is_super_admin,
            "platform authority changed"
        );

        Ok(updated)
    }

    /// Whether this account is the only super admin left.
    async fn is_last_super_admin(&self, user_id: Uuid) -> Result<bool, UserAdminError> {
        let mut page = 1;
        let mut others = 0usize;

        loop {
            let params = PaginationParams {
                page,
                per_page: 100,
            };
            let batch = self.users.list(&params).await?;

            others += batch
                .data
                .iter()
                .filter(|candidate| candidate.is_super_admin && candidate.id != user_id)
                .count();

            // One other operator is already the answer; stopping early keeps a
            // large installation from walking every account.
            if others > 0 {
                return Ok(false);
            }

            if batch.data.len() < batch.per_page as usize || page >= batch.total_pages() {
                return Ok(true);
            }

            page += 1;
        }
    }
}
