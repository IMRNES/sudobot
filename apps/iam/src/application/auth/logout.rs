//! Ending sessions.
//!
//! Split from `refresh` so the two directions stay readable: this file revokes,
//! `refresh` rotates.

pub use super::refresh::{LogoutCommand, LogoutUseCase};
