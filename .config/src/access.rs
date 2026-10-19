use anyhow::Result;

/// Who may open the dashboard, and in which guild.
///
/// The dashboard is single-tenant by decision: one Discord guild, and the operator
/// decides access by handing out a Discord role. That replaces the previous model,
/// where every login imported the caller's whole guild list and decided per guild
/// from Discord permissions — a model that needed the `guilds` OAuth scope and a
/// membership table, and that could only ever answer "which of your servers",
/// never "are you staff here".
///
/// Two ways in, both explicit:
///  - the caller holds one of `role_ids` in `guild_id`;
///  - the caller's id is in `allowlist_user_ids`, for the accounts that must get in
///    before any role exists, or when the role was never assigned.
///
/// No environment variable here is optional in the "silently permissive" sense: an
/// installation with no roles configured and no allowlist refuses everyone, which
/// is the safe reading of an empty configuration.
#[derive(Debug, Clone)]
pub struct DashboardAccessConfig {
    /// The one guild the dashboard administers.
    pub guild_id: u64,
    /// Discord role ids that grant access.
    pub role_ids: Vec<u64>,
    /// User ids allowed regardless of roles. Snowflakes, matched on Discord's id.
    pub allowlist_user_ids: Vec<u64>,
}

impl DashboardAccessConfig {
    /// Reads the whitelist, or `None` when no guild is configured.
    ///
    /// `None` rather than an error because the bot binary parses the same
    /// environment and has no dashboard: making the guild mandatory would stop a
    /// bot-only deployment from starting. The API checks for `None` and refuses
    /// logins with a message naming the missing variable.
    pub fn from_env_optional() -> Result<Option<Self>> {
        let raw = crate::optional("DASHBOARD_GUILD_ID", "");
        if raw.trim().is_empty() {
            return Ok(None);
        }

        Self::from_env().map(Some)
    }

    pub fn from_env() -> Result<Self> {
        let guild_id = crate::required("DASHBOARD_GUILD_ID")?
            .trim()
            .parse::<u64>()
            .map_err(|error| anyhow::anyhow!("`DASHBOARD_GUILD_ID` is not a snowflake: {error}"))?;

        let role_ids = parse_snowflake_list("DASHBOARD_WHITELIST_ROLE_IDS")?;
        let allowlist_user_ids = parse_snowflake_list("DASHBOARD_ALLOWLIST_USER_IDS")?;

        // A misconfigured role list would refuse every legitimate user with the
        // same message a wrong role gives, which is indistinguishable from "you
        // are not staff" and so never gets investigated. Say it at startup.
        if role_ids.is_empty() && allowlist_user_ids.is_empty() {
            tracing::warn!(
                guild_id,
                "`DASHBOARD_WHITELIST_ROLE_IDS` and `DASHBOARD_ALLOWLIST_USER_IDS` are both \
                 empty: nobody can sign in to the dashboard"
            );
        }

        Ok(Self {
            guild_id,
            role_ids,
            allowlist_user_ids,
        })
    }

    /// Whether this Discord user may use the dashboard.
    ///
    /// `member_roles` is what Discord reports for the user in [`guild_id`], and is
    /// `None` when the user is not in the guild at all. The allowlist is checked
    /// first so an operator account keeps working even if its role is stripped.
    pub fn permits(&self, discord_user_id: u64, member_roles: Option<&[u64]>) -> bool {
        if self.allowlist_user_ids.contains(&discord_user_id) {
            return true;
        }

        let Some(member_roles) = member_roles else {
            return false;
        };

        member_roles.iter().any(|role| self.role_ids.contains(role))
    }
}

/// Reads a comma-separated snowflake list, treating blank entries as absent.
///
/// Blank entries are what a trailing comma produces, and `.env` files written by
/// hand collect them. They are not "role 0" — they are nothing, and refusing to
/// start over a stray comma would be worse than ignoring it.
fn parse_snowflake_list(key: &str) -> Result<Vec<u64>> {
    crate::optional(key, "")
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry.parse::<u64>().map_err(|error| {
                anyhow::anyhow!("`{key}` entry `{entry}` is not a snowflake: {error}")
            })
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn config(roles: &[u64], allowlist: &[u64]) -> DashboardAccessConfig {
        DashboardAccessConfig {
            guild_id: 1,
            role_ids: roles.to_vec(),
            allowlist_user_ids: allowlist.to_vec(),
        }
    }

    #[test]
    fn a_whitelisted_role_permits_access() {
        let access = config(&[10, 20], &[]);
        assert!(access.permits(500, Some(&[20, 99])));
    }

    #[test]
    fn holding_no_whitelisted_role_is_refused() {
        let access = config(&[10, 20], &[]);
        assert!(!access.permits(500, Some(&[30, 40])));
    }

    /// Not in the guild means Discord gave no roles at all. That is a refusal, and
    /// it must not be confused with "in the guild with no matching role" — both
    /// refuse, but only one is worth telling the user to ask for the role.
    #[test]
    fn a_non_member_is_refused() {
        assert!(!config(&[10], &[]).permits(500, None));
    }

    /// The allowlist exists for the operator accounts that must get in before the
    /// role is assigned, and it has to work even when Discord reports no roles.
    #[test]
    fn the_allowlist_wins_over_missing_roles() {
        assert!(config(&[10], &[500]).permits(500, None));
        assert!(config(&[10], &[500]).permits(500, Some(&[])));
    }

    /// An installation with nothing configured refuses everyone. The tempting
    /// alternative — an empty whitelist meaning "let anyone in" — would turn a
    /// forgotten environment variable into an open dashboard.
    #[test]
    fn an_empty_configuration_refuses_everyone() {
        let access = config(&[], &[]);
        assert!(!access.permits(500, Some(&[1, 2, 3])));
        assert!(!access.permits(500, None));
    }
}
