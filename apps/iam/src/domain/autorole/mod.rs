pub mod entity;
pub mod errors;
pub mod repository;

pub use entity::{AutoRole, AutoRolePlan, AutoRoleTrigger, PendingGrant};
pub use errors::{AutoRoleError, ensure_due, parse_trigger, validate_delay};
pub use repository::AutoRoleRepository;
