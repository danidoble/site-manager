//! Per-section page builders. Each builds its body + header actions and owns
//! the widgets the main event loop needs to update.

pub mod backups;
pub mod dashboard;
pub mod diagnostics;
pub mod dns;
pub mod health;
pub mod logs;
pub mod new_site;
pub mod settings;
pub mod sites;
pub mod ssl;
pub mod templates;
