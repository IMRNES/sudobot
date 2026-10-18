use crate::presentation::error::AppError;
use axum::Json;
use axum::extract::{FromRequest, Request};
use serde::de::DeserializeOwned;

/// A JSON body that is validated before it reaches a handler.
///
/// `zod-rs` generates an inherent `schema()`/`validate_and_parse()` on the deriving
/// type rather than implementing a trait axum could bound on, so this pairs a small
/// local trait with `Json` instead of pretending a generic extractor exists. The
/// cost is one line per request type (see the macro below); the benefit is that a
/// handler body only ever sees a value that already passed validation.
pub trait ValidatedRequest: DeserializeOwned + Sized {
    /// Validates an already-deserialized value, reporting every problem it finds.
    ///
    /// Returns `(field, message)` pairs so the response can name the offending
    /// field instead of echoing a schema dump.
    fn validate(value: &serde_json::Value) -> Result<(), Vec<(String, String)>>;
}

/// Implements [`ValidatedRequest`] for a type deriving `zod_rs::ZodSchema`.
///
/// ```ignore
/// validated_request!(CreateUserRequest);
/// ```
#[macro_export]
macro_rules! validated_request {
    ($ty:ty) => {
        impl $crate::presentation::extract::ValidatedRequest for $ty {
            fn validate(value: &serde_json::Value) -> Result<(), Vec<(String, String)>> {
                // The derive produces `schema()` returning an unboxed schema value,
                // which is validated directly rather than through
                // `validate_and_parse` — we already hold the value and only need
                // the verdict, not a second deserialization.
                let schema = <$ty>::schema();
                match schema.safe_parse(value) {
                    Ok(_) => Ok(()),
                    Err(result) => Err(result
                        .issues
                        .iter()
                        .map(|issue| {
                            let field = if issue.path.is_empty() {
                                "body".to_string()
                            } else {
                                issue.path.join(".")
                            };
                            (field, issue.error.to_string())
                        })
                        .collect()),
                }
            }
        }
    };
}

/// The extractor handlers actually use.
///
/// Runs in two stages: `serde` decides whether the body is the right *shape*, then
/// the schema decides whether the *values* are acceptable. Separating them is what
/// lets a malformed body produce "expected a string for `name`" while a valid shape
/// with a bad value produces "name: too small".
pub struct ValidatedJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: ValidatedRequest,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<serde_json::Value>::from_request(req, state)
            .await
            .map_err(|error| AppError::BadRequest(format!("malformed JSON body: {error}")))?;

        if let Err(issues) = T::validate(&value) {
            return Err(crate::presentation::error::validation_failed(&issues));
        }

        // The shape and the values are both known good, so this deserialization
        // cannot fail for a reason the client caused.
        let parsed = serde_json::from_value::<T>(value)
            .map_err(|error| AppError::BadRequest(format!("unexpected body shape: {error}")))?;

        Ok(ValidatedJson(parsed))
    }
}
