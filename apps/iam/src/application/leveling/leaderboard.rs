//! The guild leaderboard.

use tracing::debug;

use crate::domain::errors::{Page, PaginationParams};
use crate::domain::leveling::{LevelProfile, LevelingError, LevelingRepository};

/// One row of the leaderboard.
///
/// A view rather than the profile entity: the leaderboard shows a position, and
/// `LevelProfile` has no rank field to carry one.
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardRow {
    /// The member's position in the whole guild, not in the page.
    pub rank: u64,
    pub discord_user_id: u64,
    /// The member's name, when the bot has cached one.
    ///
    /// `None` is honest rather than a placeholder: a member who has never spoken
    /// since the cache existed has no name recorded, and the dashboard falls back
    /// to the id. Inventing a name here would be worse than showing none.
    pub display_name: Option<String>,
    pub level: i32,
    pub xp: i64,
    pub total_xp: i64,
    pub message_count: i64,
    /// Share of the way to the next level, for the progress bar.
    pub progress: f64,
}

pub struct LevelLeaderboardUseCase<L> {
    leveling: L,
}

impl<L: LevelingRepository> LevelLeaderboardUseCase<L> {
    pub fn new(leveling: L) -> Self {
        Self { leveling }
    }

    /// One page of the guild's leaderboard.
    ///
    /// Two things keep this from being quadratic. The rank of a row is its
    /// position in the ordered result — `offset + index + 1` — so a page of twenty
    /// costs one query rather than twenty `rank_of` calls. And the curve is read
    /// once for the page rather than once per row, because it is the same curve for
    /// everyone in the guild.
    pub async fn execute(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<LeaderboardRow>, LevelingError> {
        // Read before the page so an empty page costs no config read at all.
        let page = self.leveling.list_leaderboard(guild_id, params).await?;

        if page.data.is_empty() {
            return Ok(Page::new(Vec::new(), params, page.total));
        }

        let config = self.leveling.find_config(guild_id).await?;
        let curve = config.curve;
        let offset = params.offset();

        let mut rows: Vec<LeaderboardRow> = page
            .data
            .iter()
            .enumerate()
            .map(|(index, profile)| row_for(profile, offset + index as u64 + 1, curve))
            .collect();

        // One lookup for the whole page. A member with no cached name keeps `None`
        // and the dashboard shows their id, which is the honest answer.
        let ids: Vec<u64> = rows.iter().map(|row| row.discord_user_id).collect();
        let names = self.leveling.member_names(guild_id, &ids).await?;
        for row in &mut rows {
            row.display_name = names.get(&row.discord_user_id).cloned();
        }

        debug!(
            guild_id,
            page = params.page,
            per_page = params.per_page,
            rows = rows.len(),
            total = page.total,
            "leaderboard page assembled"
        );

        Ok(Page::new(rows, params, page.total))
    }

    /// One member's position, for the "you are #47" line on their profile card.
    ///
    /// Separate from [`Self::execute`] because it is a different question with a
    /// different cost: the repository counts how many members are ahead, which is
    /// one query however deep the member is.
    pub async fn rank_of(&self, guild_id: u64, discord_user_id: u64) -> Result<u64, LevelingError> {
        Ok(self.leveling.rank_of(guild_id, discord_user_id).await?)
    }
}

/// Turns a profile into a row.
///
/// `progress` is the entity's own calculation — it clamps to `0.0..=1.0` and
/// handles a level whose step is zero — so the percentage the dashboard draws and
/// the one the bot prints cannot drift apart.
fn row_for(
    profile: &LevelProfile,
    rank: u64,
    curve: crate::domain::leveling::LevelCurve,
) -> LeaderboardRow {
    LeaderboardRow {
        rank,
        discord_user_id: profile.discord_user_id,
        display_name: None,
        level: profile.level,
        xp: profile.xp,
        total_xp: profile.total_xp,
        message_count: profile.message_count,
        progress: profile.progress(curve),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::application::leveling::test_support::{
        FakeLevelingRepository, LevelingCall, profile,
    };
    use crate::domain::leveling::LevelCurve;

    fn params(page: u32, per_page: u32) -> PaginationParams {
        PaginationParams { page, per_page }
    }

    #[tokio::test]
    async fn ranks_are_absolute_positions_across_pages() {
        let leveling = FakeLevelingRepository::new();
        for (user, xp) in [(1, 900), (2, 800), (3, 700), (4, 600)] {
            leveling.seed_profile(user, xp, 0, 0);
        }

        let second_page = LevelLeaderboardUseCase::new(leveling.clone())
            .execute(7, &params(2, 2))
            .await
            .expect("a page of the leaderboard");

        assert_eq!(second_page.data.len(), 2);
        assert_eq!(second_page.total, 4);
        assert_eq!(
            second_page
                .data
                .iter()
                .map(|row| (row.rank, row.discord_user_id))
                .collect::<Vec<_>>(),
            vec![(3, 3), (4, 4)],
            "page two starts at rank 3, not at rank 1"
        );
    }

    #[tokio::test]
    async fn no_rank_lookup_is_made_per_row() {
        let leveling = FakeLevelingRepository::new();
        for user in 1..=5 {
            leveling.seed_profile(user, user as i64 * 100, 0, 0);
        }

        LevelLeaderboardUseCase::new(leveling.clone())
            .execute(7, &params(1, 5))
            .await
            .expect("a full page");

        assert!(
            !leveling
                .calls()
                .iter()
                .any(|call| matches!(call, LevelingCall::RankOf { .. })),
            "five rows must not cost five rank queries"
        );
        assert_eq!(
            leveling
                .calls()
                .iter()
                .filter(|call| matches!(call, LevelingCall::FindConfig { .. }))
                .count(),
            1,
            "the curve is read once for the page, not once per row"
        );
    }

    #[tokio::test]
    async fn progress_is_the_profiles_own_calculation() {
        let leveling = FakeLevelingRepository::new();
        // Level 0 on the mrn curve needs 100 xp; 50 is halfway.
        leveling.seed_profile(1, 50, 0, 0);

        let page = LevelLeaderboardUseCase::new(leveling.clone())
            .execute(7, &params(1, 10))
            .await
            .expect("one row");

        let expected = profile(1, 50, 0, 0).progress(LevelCurve::Mrn);
        assert!((page.data[0].progress - expected).abs() < f64::EPSILON);
        assert!((page.data[0].progress - 0.5).abs() < 0.0001);
    }

    #[tokio::test]
    async fn progress_follows_the_guilds_curve() {
        let leveling = FakeLevelingRepository::new();
        leveling.set_curve(7, LevelCurve::Flat);
        // Flat costs 100 per level as well, but the point is that the config's
        // curve is the one used, not a hard-coded default.
        leveling.seed_profile(1, 25, 0, 0);

        let page = LevelLeaderboardUseCase::new(leveling.clone())
            .execute(7, &params(1, 10))
            .await
            .expect("one row");

        let expected = profile(1, 25, 0, 0).progress(LevelCurve::Flat);
        assert!((page.data[0].progress - expected).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn an_empty_guild_reads_no_config() {
        let leveling = FakeLevelingRepository::new();

        let page = LevelLeaderboardUseCase::new(leveling.clone())
            .execute(7, &params(1, 10))
            .await
            .expect("an empty page is not an error");

        assert!(page.data.is_empty());
        assert!(
            !leveling
                .calls()
                .iter()
                .any(|call| matches!(call, LevelingCall::FindConfig { .. })),
            "no rows means no curve to look up"
        );
    }

    #[tokio::test]
    async fn a_row_carries_the_members_figures() {
        let leveling = FakeLevelingRepository::new();
        leveling.seed_profile(9, 340, 12, 900);

        let page = LevelLeaderboardUseCase::new(leveling.clone())
            .execute(7, &params(1, 10))
            .await
            .expect("one row");

        let row = page.data.first().expect("one row");
        assert_eq!(row.rank, 1);
        assert_eq!(row.discord_user_id, 9);
        assert_eq!(row.level, 12);
        assert_eq!(row.xp, 340);
        assert_eq!(row.message_count, 900);
    }

    /// The leaderboard must show the name the bot recorded, so the dashboard has
    /// something real to print next to a member.
    #[tokio::test]
    async fn a_row_carries_the_recorded_name() {
        let leveling = FakeLevelingRepository::new();
        leveling.seed_profile(1, 500, 0, 0);
        leveling
            .record_member_name(7, 1, "asepharyana", false)
            .await
            .expect("a name on record");

        let page = LevelLeaderboardUseCase::new(leveling.clone())
            .execute(7, &params(1, 10))
            .await
            .expect("one row");

        assert_eq!(page.data[0].display_name.as_deref(), Some("asepharyana"));
    }

    /// A member nobody has a name for keeps `None`: the caller falls back to the id
    /// rather than inventing a name the server never gave.
    #[tokio::test]
    async fn a_member_with_no_recorded_name_stays_none() {
        let leveling = FakeLevelingRepository::new();
        leveling.seed_profile(2, 100, 0, 0);

        let page = LevelLeaderboardUseCase::new(leveling.clone())
            .execute(7, &params(1, 10))
            .await
            .expect("one row");

        assert_eq!(page.data[0].display_name, None);
    }

    #[tokio::test]
    async fn rank_of_asks_the_repository() {
        let leveling = FakeLevelingRepository::new();
        leveling.seed_profile(1, 900, 0, 0);
        leveling.seed_profile(2, 800, 0, 0);

        let rank = LevelLeaderboardUseCase::new(leveling.clone())
            .rank_of(7, 2)
            .await
            .expect("a rank");

        assert_eq!(rank, 2);
        assert_eq!(
            leveling
                .calls()
                .iter()
                .filter(|call| matches!(call, LevelingCall::RankOf { .. }))
                .count(),
            1
        );
    }
}
