//! The HTTP boundary.
//!
//! Everything here is an adapter: extract a request, call a use case, render the
//! result. Handlers own no rules — if a handler contains an `if` that decides
//! whether something is allowed, that decision belongs in the domain.
//!
//! The one thing this layer does own is the mapping from domain failures to HTTP
//! status codes, and it owns it in exactly one place ([`AppError`]) so the API's
//! error vocabulary cannot drift between routes.

pub mod dto;
pub mod error;
pub mod extract;
pub mod handlers;
pub mod middleware;
pub mod router;
pub mod state;
pub mod validation;

pub use error::AppError;
pub use router::build_router;
pub use state::AppState;
