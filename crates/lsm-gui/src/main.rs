//! `local-site-manager-gui` — GTK4 + libadwaita front end.
//!
//! Architecture: widgets live on the GTK main thread. Heavy [`lsm_core::App`]
//! calls run on worker threads and ship plain-data results back over an
//! `mpsc` channel, polled on the main thread to update the UI.

mod ui;

use gtk4::prelude::*;
use libadwaita as adw;
use tracing::{error, info};

fn main() -> glib::ExitCode {
    let paths = lsm_core::Paths::at(lsm_core::Paths::default_root());
    let _ = paths.ensure_dirs();
    let _log_guard = lsm_core::logs::init(&paths.logs);
    std::panic::set_hook(Box::new(|panic| {
        error!("gui panic: {panic}");
        eprintln!("gui panic: {panic}");
    }));

    gtk4::init().expect("gtk init");
    info!("starting Local Site Manager GUI");
    let app = adw::Application::builder()
        .application_id("local.lsm.SiteManager")
        .build();

    app.connect_activate(ui::build);

    // Run with an empty argv slice so GTK does not parse our CLI flags.
    let empty: &[&str] = &[];
    app.run_with_args(empty)
}
