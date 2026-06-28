//! GUI construction + event handling (GNOME-style two-pane).
//!
//! Widgets live on the GTK main thread. Heavy [`lsm_core::App`] calls run on
//! worker threads and ship plain-data results back over an `mpsc` channel,
//! polled on the main thread to update the UI.

pub mod about;
pub mod pages;
pub mod widgets;

use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use glib::ControlFlow;
use gtk4 as gtk;
use libadwaita as adw;
use libadwaita::prelude::*;

use lsm_core::domain::{
    BackupEntry, Ca, DiagnosticResult, HealthCheck, Site, SslCertificate, Status,
};

/// Events shipped from worker threads to the UI.
#[derive(Debug)]
pub enum Event {
    Status(Status),
    Sites(Vec<Site>),
    Certs(Vec<SslCertificate>),
    CaInfo(Option<Ca>),
    Diagnostics(Vec<DiagnosticResult>),
    Backups(Vec<BackupEntry>),
    Health(Vec<(Site, Option<HealthCheck>)>),
    DnsGuides {
        dnsmasq: String,
        hosts: String,
        wildcards: String,
    },
    Log {
        source: String,
        text: String,
    },
    TimerStatus(String),
    SslBusy(bool, String),
    Toast(String),
    Error(String),
}

/// Shared context handed to every page builder: a channel in, the main window,
/// and the toast overlay.
#[derive(Clone)]
pub struct AppCtx {
    pub sender: Sender<Event>,
    pub window: adw::ApplicationWindow,
    pub toast: adw::ToastOverlay,
}

impl AppCtx {
    /// Run a worker `FnOnce` on a background thread and forward its `Event`.
    pub fn spawn<F: FnOnce() -> Event + Send + 'static>(&self, f: F) {
        let s = self.sender.clone();
        std::thread::spawn(move || {
            let _ = s.send(f());
        });
    }

    /// Show a transient toast.
    pub fn toast(&self, msg: &str) {
        self.toast.add_toast(adw::Toast::new(msg));
    }
}

/// (stack name, display title, sidebar icon).
const SECTIONS: &[(&str, &str, &str)] = &[
    ("dashboard", "Dashboard", "go-home-symbolic"),
    ("sites", "Sites", "applications-internet-symbolic"),
    ("templates", "Templates", "folder-documents-symbolic"),
    ("ssl", "SSL", "system-lock-screen-symbolic"),
    ("dns", "DNS", "network-server-symbolic"),
    ("health", "Health", "network-wireless-symbolic"),
    ("diagnostics", "Diagnostics", "system-run-symbolic"),
    ("logs", "Logs", "utilities-terminal-symbolic"),
    ("backups", "Backups", "drive-harddisk-symbolic"),
    ("settings", "Settings", "preferences-system-symbolic"),
];

/// Called by the application on activate.
pub fn build(app: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Local Site Manager")
        .default_width(1080)
        .default_height(720)
        .build();

    // Accent + component CSS.
    widgets::load_css(&<adw::ApplicationWindow as gtk::prelude::WidgetExt>::display(&window));

    // Force minimize + maximize + close window controls on every desktop.
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_decoration_layout(Some(":minimize,maximize,close"));
    }

    let (sender, receiver) = mpsc::channel::<Event>();
    let toast = adw::ToastOverlay::new();
    let stack = adw::ViewStack::new();

    let ctx = AppCtx {
        sender: sender.clone(),
        window: window.clone(),
        toast: toast.clone(),
    };

    // ---- build pages (each owns its updatable widgets) ----
    let dashboard = pages::dashboard::DashboardPage::build(&ctx);
    let sites = pages::sites::SitesPage::build(&ctx);
    let templates = pages::templates::TemplatesPage::build(&ctx);
    let ssl = pages::ssl::SslPage::build(&ctx);
    let dns = pages::dns::DnsPage::build(&ctx);
    let health = pages::health::HealthPage::build(&ctx);
    let diag = pages::diagnostics::DiagPage::build(&ctx);
    let logs = pages::logs::LogsPage::build(&ctx);
    let backups = pages::backups::BackupsPage::build(&ctx);
    let settings = pages::settings::SettingsPage::build(&ctx);

    // ---- register pages in the content stack ----
    register(&stack, "dashboard", "Dashboard", "Overview", &dashboard);
    register(&stack, "sites", "Sites", "Managed sites & proxies", &sites);
    register(
        &stack,
        "templates",
        "Templates",
        "Project scaffolds",
        &templates,
    );
    register(&stack, "ssl", "SSL", "Certificates & local CA", &ssl);
    register(&stack, "dns", "DNS", "Resolver setup", &dns);
    register(&stack, "health", "Health", "Proxy health checks", &health);
    register(&stack, "diagnostics", "Diagnostics", "System checks", &diag);
    register(&stack, "logs", "Logs", "Live log viewer", &logs);
    register(
        &stack,
        "backups",
        "Backups",
        "Snapshots & restore",
        &backups,
    );
    register(&stack, "settings", "Settings", "Preferences", &settings);

    // ---- sidebar navigation ----
    let nav = gtk::ListBox::new();
    nav.add_css_class("navigation-sidebar");
    nav.set_selection_mode(gtk::SelectionMode::Single);
    nav.set_activate_on_single_click(true);
    for (name, title, icon) in SECTIONS {
        let row = widgets::nav_row(title, icon);
        row.set_widget_name(name);
        nav.append(&row);
    }
    {
        let stack = stack.clone();
        let ctx = ctx.clone();
        nav.connect_row_activated(move |_lb, row| {
            let name = row.widget_name();
            stack.set_visible_child_name(name.as_str());
            refresh_section(&ctx, name.as_str());
        });
    }
    if let Some(first) = nav.row_at_index(0) {
        nav.select_row(Some(&first));
    }

    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.set_show_start_title_buttons(true);
    sidebar_header.set_show_end_title_buttons(false);
    sidebar_header.set_title_widget(Some(&adw::WindowTitle::new(
        "Site Manager",
        "Local development",
    )));
    let about_btn = gtk::Button::from_icon_name("help-about-symbolic");
    about_btn.add_css_class("flat");
    about_btn.set_tooltip_text(Some("About"));
    {
        let window = window.clone();
        about_btn.connect_clicked(move |_| about::present(&window));
    }
    sidebar_header.pack_end(&about_btn);
    let sidebar_view = adw::ToolbarView::new();
    sidebar_view.add_top_bar(&sidebar_header);
    sidebar_view.set_content(Some(&nav));

    // ---- assemble split + toast ----
    toast.set_child(Some(&stack));

    let split = adw::OverlaySplitView::builder()
        .sidebar(&sidebar_view)
        .content(&toast)
        .sidebar_width_fraction(0.18)
        .min_sidebar_width(220.0)
        .max_sidebar_width(280.0)
        .build();

    window.set_content(Some(&split));
    window.present();

    // ---- dispatch events to widgets on the main thread ----
    {
        let ctx = ctx.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            while let Ok(event) = receiver.try_recv() {
                match &event {
                    Event::Status(st) => {
                        dashboard.set_status(st);
                        settings.set_status(st);
                    }
                    Event::Sites(sites_v) => {
                        dashboard.set_recent(sites_v);
                        sites.set_sites(sites_v.clone(), &ctx);
                        // Re-evaluate proxy health when sites change.
                        ctx.spawn(worker_health);
                    }
                    Event::Certs(certs) => ssl.set_certs(certs.clone(), &ctx),
                    Event::CaInfo(ca) => ssl.set_ca(ca.as_ref()),
                    Event::Diagnostics(ds) => diag.set_diagnostics(ds.clone()),
                    Event::Backups(bs) => backups.set_backups(bs, &ctx),
                    Event::Health(items) => health.set_health(items),
                    Event::DnsGuides {
                        dnsmasq,
                        hosts,
                        wildcards,
                    } => {
                        dns.set_guides(dnsmasq, hosts, wildcards);
                    }
                    Event::Log { source, text } => logs.set_log(source, text),
                    Event::TimerStatus(msg) => settings.set_timer_status(msg),
                    Event::SslBusy(busy, msg) => ssl.set_busy(*busy, msg),
                    Event::Toast(msg) | Event::Error(msg) => {
                        ctx.toast(msg);
                    }
                }
            }
            ControlFlow::Continue
        });
    }

    // Initial load.
    ctx.spawn(worker_status);
    ctx.spawn(worker_sites);
    ctx.spawn(worker_certs);
    ctx.spawn(worker_ca_info);
    ctx.spawn(worker_backups);
    ctx.spawn(worker_dns_guides);
}

fn refresh_section(ctx: &AppCtx, name: &str) {
    match name {
        "dashboard" => {
            ctx.spawn(worker_status);
            ctx.spawn(worker_sites);
        }
        "sites" => ctx.spawn(worker_sites),
        "ssl" => {
            ctx.spawn(worker_certs);
            ctx.spawn(worker_ca_info);
        }
        "dns" => ctx.spawn(worker_dns_guides),
        "health" => ctx.spawn(worker_health),
        "diagnostics" => ctx.spawn(worker_diagnostics),
        "logs" => ctx.spawn(worker_log_current),
        "backups" => ctx.spawn(worker_backups),
        "settings" => ctx.spawn(worker_status),
        _ => {}
    }
}

/// Wraps a page body in an `AdwToolbarView`/`AdwHeaderBar` shell and registers
/// it in the stack. Borrows the page (which stays owned by the caller so the
/// event loop can keep updating it).
fn register<P: Page>(stack: &adw::ViewStack, name: &str, title: &str, subtitle: &str, page: &P) {
    let shell = page_shell(title, subtitle, page.body(), page.actions());
    let p = stack.add_titled(&shell.upcast::<gtk::Widget>(), Some(name), title);
    p.set_icon_name(Some(
        SECTIONS
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, _, i)| *i)
            .unwrap_or("text-x-generic-symbolic"),
    ));
}

/// A page exposes its body widget and header-bar action buttons.
pub trait Page {
    fn body(&self) -> &gtk::Widget;
    fn actions(&self) -> Vec<gtk::Widget>;
}

fn page_shell(
    title: &str,
    subtitle: &str,
    body: &gtk::Widget,
    actions: Vec<gtk::Widget>,
) -> adw::ToolbarView {
    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(true);
    header.set_title_widget(Some(&adw::WindowTitle::new(title, subtitle)));
    for a in &actions {
        header.pack_end(a);
    }

    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&header);
    tv.set_content(Some(body));
    tv
}

impl Page for pages::dashboard::DashboardPage {
    fn body(&self) -> &gtk::Widget {
        &self.body
    }
    fn actions(&self) -> Vec<gtk::Widget> {
        self.actions.clone()
    }
}
impl Page for pages::sites::SitesPage {
    fn body(&self) -> &gtk::Widget {
        &self.body
    }
    fn actions(&self) -> Vec<gtk::Widget> {
        self.actions.clone()
    }
}
impl Page for pages::templates::TemplatesPage {
    fn body(&self) -> &gtk::Widget {
        &self.body
    }
    fn actions(&self) -> Vec<gtk::Widget> {
        self.actions.clone()
    }
}
impl Page for pages::ssl::SslPage {
    fn body(&self) -> &gtk::Widget {
        &self.body
    }
    fn actions(&self) -> Vec<gtk::Widget> {
        self.actions.clone()
    }
}
impl Page for pages::dns::DnsPage {
    fn body(&self) -> &gtk::Widget {
        &self.body
    }
    fn actions(&self) -> Vec<gtk::Widget> {
        self.actions.clone()
    }
}
impl Page for pages::health::HealthPage {
    fn body(&self) -> &gtk::Widget {
        &self.body
    }
    fn actions(&self) -> Vec<gtk::Widget> {
        self.actions.clone()
    }
}
impl Page for pages::diagnostics::DiagPage {
    fn body(&self) -> &gtk::Widget {
        &self.body
    }
    fn actions(&self) -> Vec<gtk::Widget> {
        self.actions.clone()
    }
}
impl Page for pages::logs::LogsPage {
    fn body(&self) -> &gtk::Widget {
        &self.body
    }
    fn actions(&self) -> Vec<gtk::Widget> {
        self.actions.clone()
    }
}
impl Page for pages::backups::BackupsPage {
    fn body(&self) -> &gtk::Widget {
        &self.body
    }
    fn actions(&self) -> Vec<gtk::Widget> {
        self.actions.clone()
    }
}
impl Page for pages::settings::SettingsPage {
    fn body(&self) -> &gtk::Widget {
        &self.body
    }
    fn actions(&self) -> Vec<gtk::Widget> {
        self.actions.clone()
    }
}

// ---- workers ----

pub fn worker_status() -> Event {
    match lsm_core::App::new().and_then(|a| a.status()) {
        Ok(s) => Event::Status(s),
        Err(e) => Event::Error(e.to_string()),
    }
}

pub fn worker_sites() -> Event {
    match lsm_core::App::new().and_then(|a| a.list_sites(None, 1, 500)) {
        Ok(s) => Event::Sites(s),
        Err(e) => Event::Error(e.to_string()),
    }
}

pub fn worker_certs() -> Event {
    match lsm_core::App::new().and_then(|a| a.list_certs()) {
        Ok(c) => Event::Certs(c),
        Err(e) => Event::Error(e.to_string()),
    }
}

pub fn worker_ca_info() -> Event {
    match lsm_core::App::new().and_then(|a| a.ca_info()) {
        Ok(ca) => Event::CaInfo(ca),
        Err(e) => Event::Error(e.to_string()),
    }
}

pub fn worker_diagnostics() -> Event {
    match lsm_core::App::new().and_then(|a| a.diagnose()) {
        Ok(d) => Event::Diagnostics(d),
        Err(e) => Event::Error(e.to_string()),
    }
}

pub fn worker_backups() -> Event {
    match lsm_core::App::new().and_then(|a| a.backup_list()) {
        Ok(b) => Event::Backups(b),
        Err(e) => Event::Error(e.to_string()),
    }
}

pub fn worker_health() -> Event {
    let result = lsm_core::App::new().and_then(|a| {
        let sites = a.list_sites(None, 1, 500)?;
        let mut out = Vec::new();
        for s in &sites {
            let h = a.latest_health(s.id).unwrap_or(None);
            out.push((s.clone(), h));
        }
        Ok(out)
    });
    match result {
        Ok(items) => Event::Health(items),
        Err(e) => Event::Error(e.to_string()),
    }
}

pub fn worker_dns_guides() -> Event {
    match lsm_core::App::new().map(|a| a.dns_guides("test")) {
        Ok((dnsmasq, hosts, wildcards)) => Event::DnsGuides {
            dnsmasq,
            hosts,
            wildcards,
        },
        Err(e) => Event::Error(e.to_string()),
    }
}

pub fn worker_log_current() -> Event {
    Event::Log {
        source: "app".into(),
        text: crate::ui::pages::logs::read_log_source("app"),
    }
}
