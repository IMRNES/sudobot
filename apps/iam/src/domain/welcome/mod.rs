pub mod entity;
pub mod errors;
pub mod repository;

pub use entity::{
    OptionBehavior, RolePanel, RolePanelOption, RolePanelWithOptions, TemplateContext,
    WelcomeConfig,
};
pub use errors::{
    ALLOWED_PLACEHOLDERS, WelcomeError, panel_not_found, parse_behavior, validate_button_style,
    validate_option_count, validate_template,
};
pub use repository::WelcomeRepository;
