//! Conversions shared by every repository in this module tree.
//!
//! Discord snowflakes are unsigned 64-bit and the schema stores them as `BIGINT`;
//! timestamps come back as `TIMESTAMPTZ` and the domain speaks UTC. Both
//! conversions live here so every adapter spells them the same way.

use chrono::{DateTime, Utc};
use sea_orm::prelude::DateTimeWithTimeZone;

/// A snowflake on its way into the database.
///
/// The cast preserves all 64 bits: Discord's ids use the full unsigned range, and
/// a narrowing conversion would silently alias two different guilds.
pub fn snowflake(value: u64) -> i64 {
    value as i64
}

/// A snowflake on its way out of the database.
pub fn snowflake_from_db(value: i64) -> u64 {
    value as u64
}

/// `TIMESTAMPTZ` to the domain's UTC instant.
pub fn to_utc(value: DateTimeWithTimeZone) -> DateTime<Utc> {
    value.with_timezone(&Utc)
}

/// A UTC instant to the column's type.
pub fn from_utc(value: DateTime<Utc>) -> DateTimeWithTimeZone {
    value.fixed_offset()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn snowflakes_round_trip_across_the_unsigned_range() {
        // The top bit set is the case a naive `as i32` or a signed round-trip breaks.
        for value in [
            0u64,
            1,
            u64::from(u32::MAX),
            9_223_372_036_854_775_807,
            u64::MAX,
        ] {
            assert_eq!(snowflake_from_db(snowflake(value)), value);
        }
    }

    #[test]
    fn timestamps_survive_the_round_trip() {
        let now = Utc::now();
        assert_eq!(to_utc(from_utc(now)), now);
    }
}
