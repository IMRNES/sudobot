//! Welcome, goodbye, and the button-role panels.
//!
//! Three audiences share this module: the dashboard (read and write the config,
//! edit panels), the gateway's join path ([`BuildWelcomeMessageUseCase`]), and the
//! button interaction path ([`ResolvePanelButtonUseCase`]). The last two are hot
//! and must not fail a member event, which is why they answer with `Option`
//! instead of an error wherever "there is nothing to do" is a normal outcome.

pub mod config;
pub mod message;
pub mod panels;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
mod tests;

pub use config::{GetWelcomeConfigUseCase, UpdateWelcomeConfigCommand, UpdateWelcomeConfigUseCase};
pub use message::{
    BuildWelcomeMessageQuery, BuildWelcomeMessageUseCase, WelcomeKind, WelcomeMessage,
};
pub use panels::{
    AddPanelOptionCommand, AddPanelOptionUseCase, DeletePanelOptionUseCase, GetRolePanelUseCase,
    ListRolePanelsUseCase, PanelButton, PublishRolePanelUseCase, ResolvePanelButtonQuery,
    ResolvePanelButtonUseCase, UpsertRolePanelCommand, UpsertRolePanelUseCase,
};
