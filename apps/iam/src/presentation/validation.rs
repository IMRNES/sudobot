//! Field-level rules the schemas cannot express.
//!
//! `zod-rs` covers lengths, formats and ranges. Anything that depends on more than
//! one field, or that is domain knowledge (a snowflake is not a string, a colour is
//! not any integer), is checked here so the handler stays a straight line.

use crate::presentation::error::AppError;

/// Parses a Discord snowflake from the string the client sent.
///
/// Snowflakes are strings on the wire because they exceed 2^53, so a JavaScript
/// client would silently round them if they were numbers. Parsing here, rather
/// than accepting an integer, keeps the contract honest.
pub fn parse_snowflake(value: &str, field: &str) -> Result<u64, AppError> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return Err(AppError::BadRequest(format!("{field} is required")));
    }

    // A value that is all digits but does not fit is a client bug worth naming
    // precisely — "invalid" sends people looking for a typo that is not there.
    if !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::BadRequest(format!(
            "{field} must be a discord id (digits only), got `{value}`"
        )));
    }

    trimmed
        .parse::<u64>()
        .map_err(|_| AppError::BadRequest(format!("{field} is out of range for a discord id")))
}

/// Parses an optional snowflake, treating `null`/absent as `None`.
pub fn parse_optional_snowflake(value: Option<&str>, field: &str) -> Result<Option<u64>, AppError> {
    match value {
        None => Ok(None),
        Some(raw) if raw.trim().is_empty() => Ok(None),
        Some(raw) => parse_snowflake(raw, field).map(Some),
    }
}

/// Parses the `guildId` path segment.
pub fn guild_id_from_path(raw: &str) -> Result<u64, AppError> {
    parse_snowflake(raw, "guildId")
}

/// Validates an embed colour.
///
/// Discord takes a 24-bit integer; anything outside that range renders as black,
/// which looks like a bug rather than a rejection.
pub fn validate_colour(value: i32, field: &str) -> Result<i32, AppError> {
    if !(0..=0xFF_FFFF).contains(&value) {
        return Err(AppError::BadRequest(format!(
            "{field} must be between 0 and 16777215"
        )));
    }
    Ok(value)
}

/// Validates a per-page value before it reaches the paginator.
///
/// The paginator clamps too, but a request asking for 10 000 rows should be told
/// no rather than silently given 100 — silent clamping looks like data loss.
pub fn validate_per_page(value: u32) -> Result<u32, AppError> {
    if value == 0 {
        return Err(AppError::BadRequest("per_page must be at least 1".into()));
    }
    if value > 100 {
        return Err(AppError::BadRequest("per_page must be at most 100".into()));
    }
    Ok(value)
}

/// Validates a limit parameter for the endpoints that do not paginate.
pub fn validate_limit(value: u32, max: u32, field: &str) -> Result<u32, AppError> {
    if value == 0 {
        return Err(AppError::BadRequest(format!("{field} must be at least 1")));
    }
    if value > max {
        return Err(AppError::BadRequest(format!(
            "{field} must be at most {max}"
        )));
    }
    Ok(value)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_real_snowflake() {
        assert_eq!(
            parse_snowflake("123456789012345678", "guildId").unwrap(),
            123_456_789_012_345_678
        );
    }

    #[test]
    fn rejects_a_snowflake_that_arrived_as_a_float() {
        // This is what a client sends after round-tripping it through a JS number.
        assert!(parse_snowflake("123456789012345680.0", "guildId").is_err());
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(parse_snowflake("99999999999999999999999", "guildId").is_err());
    }

    #[test]
    fn per_page_is_bounded_at_both_ends() {
        assert!(validate_per_page(0).is_err());
        assert!(validate_per_page(20).is_ok());
        assert!(validate_per_page(101).is_err());
    }

    #[test]
    fn colour_range_matches_discords_24_bit_field() {
        assert!(validate_colour(0, "color").is_ok());
        assert!(validate_colour(0xFF_FFFF, "color").is_ok());
        assert!(validate_colour(0x100_0000, "color").is_err());
    }
}
