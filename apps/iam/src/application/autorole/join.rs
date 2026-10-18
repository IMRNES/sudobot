use tracing::{info, warn};
use uuid::Uuid;

use crate::application::ports::Clock;
use crate::domain::autorole::{AutoRoleError, AutoRolePlan, AutoRoleRepository, PendingGrant};

/// A member joined: decide which roles to grant, and when.
#[derive(Debug, Clone)]
pub struct PlanJoinAutoRolesCommand {
    pub guild_id: u64,
    pub discord_user_id: u64,
    pub is_bot: bool,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

/// The join path.
///
/// This runs on a gateway event, so its contract is deliberately forgiving: a
/// member joining must never fail because of a misconfigured rule, and the role
/// grant that is already due is returned rather than left for a sweep.
pub struct PlanJoinAutoRolesUseCase<A, C> {
    auto_roles: A,
    clock: C,
}

impl<A: AutoRoleRepository, C: Clock> PlanJoinAutoRolesUseCase<A, C> {
    pub fn new(auto_roles: A, clock: C) -> Self {
        Self { auto_roles, clock }
    }

    /// Always `Ok`: a misconfigured rule is logged, not propagated, because the
    /// alternative is a member who joins the guild and gets nothing.
    pub async fn execute(
        &self,
        command: PlanJoinAutoRolesCommand,
    ) -> Result<AutoRolePlan, AutoRoleError> {
        let rules = match self.auto_roles.list_for_guild(command.guild_id).await {
            Ok(rules) => rules,
            Err(error) => {
                // The guild keeps working; the rule author sees the problem in the
                // log rather than the member seeing it as a failed join.
                warn!(
                    guild_id = command.guild_id,
                    %error,
                    "could not read auto roles for a joining member"
                );
                return Ok(AutoRolePlan {
                    immediate: Vec::new(),
                    deferred: Vec::new(),
                });
            }
        };

        // Pure decision, no I/O: `build` filters to the rules that apply to this
        // member and splits them into "now" and "later".
        let plan = AutoRolePlan::build(&rules, command.is_bot, command.joined_at);

        // The deferred half is queued so a restart between now and the due time
        // does not lose it — which is the entire point of delayed autoroles as an
        // anti-raid measure.
        for (auto_role_id, role_id, due_at) in &plan.deferred {
            let grant = PendingGrant {
                id: Uuid::new_v4(),
                guild_id: command.guild_id,
                discord_user_id: command.discord_user_id,
                auto_role_id: *auto_role_id,
                role_id: *role_id,
                due_at: *due_at,
                attempts: 0,
                last_error: None,
                created_at: command.joined_at,
            };

            // `None` means the same (rule, member) pair was already queued — a
            // rejoin, or a duplicate gateway event. Queueing it again would stack
            // two identical grants, so it is skipped rather than retried.
            match self.auto_roles.enqueue_pending(grant).await {
                Ok(Some(_)) => {}
                Ok(None) => {
                    info!(
                        guild_id = command.guild_id,
                        auto_role_id = %auto_role_id,
                        "a grant for this member is already queued"
                    );
                }
                Err(error) => {
                    warn!(
                        guild_id = command.guild_id,
                        auto_role_id = %auto_role_id,
                        %error,
                        "could not queue a delayed auto role"
                    );
                }
            }
        }

        info!(
            guild_id = command.guild_id,
            discord_user_id = command.discord_user_id,
            immediate = plan.immediate.len(),
            deferred = plan.deferred.len(),
            now = %self.clock.now(),
            "auto role plan built for a join"
        );

        Ok(plan)
    }
}
