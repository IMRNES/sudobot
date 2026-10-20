//! Request and response shapes.
//!
//! Two rules hold throughout this module, and both exist to stop internal detail
//! from becoming a public contract by accident:
//!
//! 1. **Request types never reach the domain.** A handler converts a request into
//!    the use case's command through `From`, and the conversion is where a string
//!    becomes a `Uuid`, a snowflake becomes a `u64`, and a nullable field becomes
//!    an `Option`.
//! 2. **Response types are built from domain entities**, never from ORM models. A
//!    column added for internal bookkeeping cannot appear in a response unless
//!    somebody deliberately added it here.
//!
//! Snowflakes are serialized as strings: they exceed 2^53 and a JavaScript client
//! would silently round them if they arrived as numbers.

pub mod auth;
pub mod autorole;
pub mod economy;
pub mod guild;
pub mod jail;
pub mod leveling;
pub mod member;
pub mod role;
pub mod welcome;

use serde::Serialize;

/// The envelope every list endpoint returns.
///
/// Mirrors the shape `paginator-axum` emits — `data` plus a `meta` object — because
/// the paginated handlers return that type directly rather than re-wrapping it.
/// This struct exists for the few endpoints that paginate by hand (a leaderboard
/// with a computed rank, for instance) so both spell the same JSON.
#[derive(Debug, Serialize)]
pub struct PageResponse<T> {
    pub data: Vec<T>,
    pub meta: PageMeta,
}

#[derive(Debug, Serialize)]
pub struct PageMeta {
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
}

impl PageMeta {
    pub fn new(page: u32, per_page: u32, total: u64) -> Self {
        let total_pages = if per_page == 0 {
            0
        } else {
            total.div_ceil(u64::from(per_page)) as u32
        };

        Self {
            page,
            per_page,
            total,
            total_pages,
            has_next: u64::from(page) < u64::from(total_pages),
            has_prev: page > 1,
        }
    }
}

/// Converts the domain's `Page` into the wire shape.
impl<T> From<crate::domain::errors::Page<T>> for PageResponse<T> {
    fn from(page: crate::domain::errors::Page<T>) -> Self {
        let meta = PageMeta::new(page.page, page.per_page, page.total);
        Self {
            data: page.data,
            meta,
        }
    }
}

/// The body returned by a delete, and by anything else with nothing to say.
///
/// A 204 with no body is the usual answer and handlers use that where they can.
/// This exists for the cases where a client benefits from a count — "revoked 3
/// sessions" is more useful than silence.
#[derive(Debug, Serialize)]
pub struct AffectedResponse {
    pub affected: u64,
}

/// Serializes a snowflake as a string.
///
/// Used through `#[serde(with = "...")]` on every snowflake field, so the choice is
/// visible at the field rather than buried in a custom serializer.
pub mod snowflake {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse::<u64>().map_err(serde::de::Error::custom)
    }
}

/// The same, for an optional snowflake.
pub mod optional_snowflake {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serializer.serialize_some(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error> {
        let raw = Option::<String>::deserialize(deserializer)?;
        match raw {
            None => Ok(None),
            Some(value) if value.trim().is_empty() => Ok(None),
            Some(value) => value
                .parse::<u64>()
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }
}

/// The same, for a field that distinguishes absent from explicit null.
///
/// `Option<Option<T>>` is the shape a PATCH needs — outer `None` means "leave this
/// alone", inner `None` means "clear it" — and serde's derive cannot express it:
/// its `Option` impl maps a JSON `null` to the *outer* `None`, so "clear it" and
/// "leave it" arrive identically and one of the two intents is lost. These helpers
/// deserialize by hand, which is the only way the distinction survives the wire.
pub mod triple_snowflake {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        value: &Option<Option<u64>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            None => serializer.serialize_none(),
            Some(None) => serializer.serialize_none(),
            Some(Some(value)) => serializer.serialize_some(&value.to_string()),
        }
    }

    /// Absent -> `None` (via `#[serde(default)]`); `null` -> `Some(None)`; a value
    /// -> `Some(Some(id))`.
    ///
    /// The visitor is hand-written because `Option::<Option<String>>::deserialize`
    /// does *not* preserve the distinction: serde's `Option` impl maps a JSON `null`
    /// to the outer `None`, so "clear it" and "leave it alone" collapse into the same
    /// value and the PATCH silently does nothing. Only `deserialize_option` with a
    /// `visit_none` arm keeps a present null separate from an absent key.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Option<u64>>, D::Error> {
        struct TripleOption;

        impl<'de> serde::de::Visitor<'de> for TripleOption {
            type Value = Option<Option<u64>>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a snowflake as a string, null, or nothing")
            }

            /// A present `null`: the caller wants this field cleared.
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(Some(None))
            }

            /// Same meaning as `null` for unit-shaped input.
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(Some(None))
            }

            fn visit_some<D: Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                let raw = Option::<String>::deserialize(deserializer)?;

                match raw {
                    // An empty string is what an emptied form input sends; treating it
                    // as "clear" is what makes the picker's clear button work.
                    None => Ok(Some(None)),
                    Some(value) if value.trim().is_empty() => Ok(Some(None)),
                    Some(value) => value
                        .parse::<u64>()
                        .map(|parsed| Some(Some(parsed)))
                        .map_err(serde::de::Error::custom),
                }
            }
        }

        deserializer.deserialize_option(TripleOption)
    }
}

/// The same, for a list of snowflakes.
pub mod snowflake_list {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &[u64], serializer: S) -> Result<S::Ok, S::Error> {
        let rendered: Vec<String> = value.iter().map(u64::to_string).collect();
        serializer.collect_seq(rendered)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u64>, D::Error> {
        let raw = Vec::<String>::deserialize(deserializer)?;
        raw.into_iter()
            .map(|value| value.parse::<u64>().map_err(serde::de::Error::custom))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::triple_snowflake;
    use serde::Deserialize;

    /// The three-state shape a PATCH depends on.
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Patch {
        #[serde(default, with = "triple_snowflake")]
        value: Option<Option<u64>>,
    }

    fn parse(json: &str) -> Patch {
        serde_json::from_str(json).expect("valid patch body")
    }

    #[test]
    fn an_absent_field_means_leave_it_alone() {
        // The distinction the whole type exists for: absent is not null. Collapsing
        // them would make "I only changed the bail price" silently clear the jail.
        assert_eq!(parse("{}").value, None);
    }

    #[test]
    fn an_explicit_null_means_clear_it() {
        assert_eq!(parse(r#"{"value": null}"#).value, Some(None));
    }

    #[test]
    fn a_string_snowflake_is_parsed_as_a_number() {
        // Snowflakes arrive as strings because they exceed 2^53; a client that sent
        // them as JSON numbers would have already rounded them.
        assert_eq!(
            parse(r#"{"value": "1450861658061803624"}"#).value,
            Some(Some(1_450_861_658_061_803_624))
        );
    }

    #[test]
    fn an_empty_string_is_treated_as_cleared() {
        // What an empty form input sends. Rejecting it would make the picker's
        // "clear" button an error instead of an action.
        assert_eq!(parse(r#"{"value": ""}"#).value, Some(None));
    }

    #[test]
    fn a_non_numeric_value_is_rejected() {
        assert!(serde_json::from_str::<Patch>(r#"{"value": "not-a-snowflake"}"#).is_err());
    }
}
