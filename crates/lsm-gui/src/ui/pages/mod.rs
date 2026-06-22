//! Per-section page builders. Each builds its body + header actions and owns
//! the widgets the main event loop needs to update.

pub mod dashboard;
pub mod sites;
pub mod new_site;
pub mod templates;
pub mod ssl;
pub mod dns;
pub mod health;
pub mod diagnostics;
pub mod logs;
pub mod backups;
pub mod settings;
