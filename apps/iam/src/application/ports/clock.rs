use chrono::{DateTime, Utc};
use std::future::Future;

/// The current time, as a collaborator rather than a global.
///
/// Daily rewards, XP cooldowns and delayed autoroles are all "is it time yet?"
/// questions. Reading `Utc::now()` directly inside a use case makes those rules
/// untestable; taking a clock lets a test decide that an hour has passed without
/// sleeping for one.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;

    /// Awaits until `deadline`, or returns immediately if it has passed.
    ///
    /// Exists so a long wait can be driven by a fake clock in tests instead of
    /// blocking a real one.
    fn sleep_until(&self, deadline: DateTime<Utc>) -> impl Future<Output = ()> + Send;
}

/// The clock production uses: wall time and a real timer.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    async fn sleep_until(&self, deadline: DateTime<Utc>) {
        let now = Utc::now();
        if deadline <= now {
            return;
        }

        let wait = (deadline - now)
            .to_std()
            .unwrap_or(std::time::Duration::ZERO);

        // `sleep` is not precise for long durations, and a deadline a week out
        // would overflow some platforms. Sleeping in bounded chunks keeps the
        // autorole sweep accurate without a timer per grant.
        let mut remaining = wait;
        while remaining > std::time::Duration::ZERO {
            let chunk = remaining.min(std::time::Duration::from_secs(60));
            tokio::time::sleep(chunk).await;
            remaining -= chunk;
        }
    }
}
