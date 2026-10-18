use axum::extract::{FromRequestParts, Request};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::application::ports::{Clock, TokenService, TokenType};
use crate::presentation::error::AppError;
use crate::presentation::state::AppState;

/// Who is calling, once their token has been checked.
///
/// Carries only the internal user id: the Discord id, the name and everything else
/// are what the handler looks up if it needs them, so a stolen extension cannot
/// leak more than the caller already knew.
#[derive(Debug, Clone, Copy)]
pub struct AuthenticatedUser {
    pub id: Uuid,
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Inserted by `attach_auth` when the request carried a usable token. Absent
        // means either no token or one that failed verification, and the client's
        // answer to both is the same: sign in, or refresh and retry.
        parts
            .extensions
            .get::<AuthenticatedUser>()
            .copied()
            .ok_or_else(|| {
                AppError::Unauthorized("sign in to continue, or refresh your access token".into())
            })
    }
}

/// Reads a bearer token when one is present, and never rejects the request.
///
/// Deliberately *not* an enforcing layer. Which routes need a session is decided by
/// [`AuthenticatedUser`]'s extractor: a handler that asks for the caller gets a 401
/// without a valid token, and a handler that does not is free to answer anonymous
/// callers. Enforcing in a layer instead means the router has to be split into
/// public and private tiers, and every route then has to be filed under the right
/// one — which is how `/health` and the login start end up returning 401.
///
/// A malformed or expired token is ignored rather than refused here, so a stale
/// token in a browser's storage cannot break a public endpoint. The handler that
/// needs the identity reports the real reason through the extractor.
pub async fn attach_auth(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = bearer_token(&request) else {
        return next.run(request).await;
    };

    let now = state.clock.now();

    match state
        .token_service
        .verify(&token, TokenType::Access, now)
        .await
    {
        Ok(claims) => {
            request
                .extensions_mut()
                .insert(AuthenticatedUser { id: claims.subject });
        }
        Err(error) => {
            // Logged, not returned: whether this matters is the handler's call.
            tracing::debug!(%error, "ignoring an unusable access token on this route");
        }
    }

    next.run(request).await
}

/// Extracts the token from an `Authorization: Bearer <token>` header.
///
/// The scheme match is case-insensitive because RFC 7235 says it is, and clients
/// do send `bearer`.
fn bearer_token(request: &Request) -> Option<String> {
    let header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;

    let (scheme, token) = header.split_once(' ')?;

    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    let token = token.trim();
    if token.is_empty() {
        return None;
    }

    Some(token.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn request_with(header: Option<&str>) -> Request {
        let mut builder = Request::builder().uri("/");
        if let Some(value) = header {
            builder = builder.header(axum::http::header::AUTHORIZATION, value);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn reads_a_bearer_token() {
        let request = request_with(Some("Bearer abc.def.ghi"));
        assert_eq!(bearer_token(&request).as_deref(), Some("abc.def.ghi"));
    }

    #[test]
    fn the_scheme_is_case_insensitive() {
        let request = request_with(Some("bearer abc"));
        assert_eq!(bearer_token(&request).as_deref(), Some("abc"));
    }

    #[test]
    fn refuses_another_scheme() {
        // A Basic header must not be mistaken for a bearer token.
        let request = request_with(Some("Basic dXNlcjpwYXNz"));
        assert!(bearer_token(&request).is_none());
    }

    #[test]
    fn refuses_an_empty_token() {
        let request = request_with(Some("Bearer "));
        assert!(bearer_token(&request).is_none());
    }

    #[test]
    fn refuses_a_missing_header() {
        assert!(bearer_token(&request_with(None)).is_none());
    }
}
