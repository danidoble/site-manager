//! Settings: appearance, runtime detection, provider + config info.

use gtk4 as gtk;
use gtk::prelude::*;
use gtk::StringList;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::widgets::{margin_all, scrolled};
use crate::ui::{worker_status, AppCtx, Event};
use lsm_core::domain::Status;

pub struct SettingsPage {
    pub body: gtk::Widget,
    pub actions: Vec<gtk::Widget>,
    timer_status: gtk::Label,
}

impl SettingsPage {
    pub fn build(ctx: &AppCtx) -> Self {
        let inner = gtk::Box::new(gtk::Orientation::Vertical, 18);
        margin_all(&inner, 12);

        // Appearance.
        let appearance = adw::PreferencesGroup::new();
        appearance.set_title("Appearance");
        let mode_row = adw::ActionRow::builder().title("Color scheme").subtitle("Light, dark, or follow system").build();
        let modes = StringList::new(&["System", "Light", "Dark"]);
        let mode_dd = gtk::DropDown::builder().model(&modes).build();
        let mode_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        mode_box.set_valign(gtk::Align::Center);
        mode_box.append(&mode_dd);
        mode_row.add_suffix(&mode_box);
        mode_dd.connect_selected_notify(|dd| {
            let sm = adw::StyleManager::default();
            sm.set_color_scheme(match dd.selected() {
                1 => adw::ColorScheme::PreferLight,
                2 => adw::ColorScheme::PreferDark,
                _ => adw::ColorScheme::Default,
            });
        });
        appearance.add(&mode_row);
        inner.append(&appearance);

        // Runtime detection + provider info (from config).
        let cfg = lsm_core::App::new().map(|a| (a.config.clone(), a.paths.root.clone()));
        if let Ok((config, root)) = cfg {
            let info = adw::PreferencesGroup::new();
            info.set_title("System");
            info.add(&kv_row("Storage root", &root.display().to_string()));
            info.add(&kv_row("Web server", &config.web_server));
            info.add(&kv_row("Cert provider", &format!("{:?}", config.cert_provider)));
            info.add(&kv_row("WWW root", &config.www_root));
            info.add(&kv_row("API port", &config.api_port.to_string()));
            if !config.php_versions.is_empty() {
                info.add(&kv_row("PHP versions", &config.php_versions.join(", ")));
            }
            inner.append(&info);
        }

        // Providers / runtime note.
        let providers = adw::PreferencesGroup::new();
        providers.set_title("Providers");
        providers.set_description(Some(
            "Web server, certificate and runtime providers are auto-detected. \
             Import/export of full configuration is covered by the Backups section.",
        ));
        inner.append(&providers);

        let timer_status = gtk::Label::new(Some("Not checked"));
        timer_status.add_css_class("dim-more");
        timer_status.add_css_class("monospace");
        timer_status.set_xalign(0.0);

        let timer = adw::PreferencesGroup::new();
        timer.set_title("Auto-renew timer");
        timer.set_description(Some(
            "Installs and controls the systemd timer that renews local SSL certificates automatically.",
        ));
        timer.add(&timer_status_row(&timer_status));
        timer.add(&timer_button_row(ctx, &timer_status));
        inner.append(&timer);

        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(760);
        clamp.set_child(Some(&inner));

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&scrolled(&clamp));

        Self {
            body: root.upcast(),
            actions: vec![],
            timer_status,
        }
    }

    /// Refresh runtime info from a status snapshot (PHP versions etc.).
    pub fn set_status(&self, _st: &Status) {
        // Config is read at build; live PHP detection could refresh here later.
        if self.timer_status.text().is_empty() {
            self.timer_status.set_text("Not checked");
        }
    }

    pub fn set_timer_status(&self, msg: &str) {
        self.timer_status.set_text(msg);
    }
}

fn timer_status_row(label: &gtk::Label) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title("Timer status")
        .subtitle("local-site-manager.timer")
        .build();
    row.add_suffix(label);
    row
}

fn timer_button_row(ctx: &AppCtx, status_label: &gtk::Label) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title("Controls")
        .subtitle("Install, enable, disable, restart or check without opening a terminal")
        .build();
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    buttons.set_valign(gtk::Align::Center);

    for (label, action, icon) in [
        ("Install", TimerAction::Install, "document-save-symbolic"),
        ("Enable", TimerAction::Enable, "media-playback-start-symbolic"),
        ("Disable", TimerAction::Disable, "media-playback-stop-symbolic"),
        ("Restart", TimerAction::Restart, "view-refresh-symbolic"),
        ("Status", TimerAction::Status, "dialog-information-symbolic"),
    ] {
        let btn = gtk::Button::builder().label(label).icon_name(icon).build();
        btn.add_css_class("flat");
        {
            let ctx = ctx.clone();
            let status_label = status_label.clone();
            btn.connect_clicked(move |_| run_timer_action(&ctx, &status_label, action));
        }
        buttons.append(&btn);
    }

    row.add_suffix(&buttons);
    row
}

#[derive(Clone, Copy)]
enum TimerAction {
    Install,
    Enable,
    Disable,
    Restart,
    Status,
}

fn run_timer_action(ctx: &AppCtx, status_label: &gtk::Label, action: TimerAction) {
    status_label.set_text("Working...");
    let sender = ctx.sender.clone();
    std::thread::spawn(move || {
        let result = lsm_core::App::new().and_then(|app| match action {
            TimerAction::Install => app.install_auto_renew_timer().map(|r| r.message),
            TimerAction::Enable => {
                app.systemctl("enable", "local-site-manager.timer")?;
                app.systemctl("start", "local-site-manager.timer")?;
                Ok("Auto-renew timer enabled and started".to_string())
            }
            TimerAction::Disable => {
                let _ = app.systemctl_capture("stop", "local-site-manager.timer");
                app.systemctl("disable", "local-site-manager.timer")?;
                Ok("Auto-renew timer disabled".to_string())
            }
            TimerAction::Restart => app
                .systemctl("restart", "local-site-manager.timer")
                .map(|_| "Auto-renew timer restarted".to_string()),
            TimerAction::Status => timer_status(&app),
        });

        match result {
            Ok(msg) => {
                let _ = sender.send(Event::TimerStatus(msg.clone()));
                let _ = sender.send(Event::Toast(msg.clone()));
            }
            Err(e) => {
                let msg = e.to_string();
                let _ = sender.send(Event::TimerStatus(msg.clone()));
                let _ = sender.send(Event::Error(msg.clone()));
            }
        }
        let _ = sender.send(worker_status());
    });
}

fn timer_status(app: &lsm_core::App) -> lsm_core::Result<String> {
    let enabled = app.systemctl_capture("is-enabled", "local-site-manager.timer")?;
    let active = app.systemctl_capture("is-active", "local-site-manager.timer")?;
    let enabled_state = status_text(&enabled, "disabled or not installed");
    let active_state = status_text(&active, "inactive");
    Ok(format!("enabled: {enabled_state} · active: {active_state}"))
}

fn status_text(res: &lsm_core::privileged::PrivilegedResult, fallback: &str) -> String {
    let stdout = res.stdout.trim();
    if !stdout.is_empty() {
        stdout.to_string()
    } else if res.success {
        "ok".to_string()
    } else {
        fallback.to_string()
    }
}

fn kv_row(k: &str, v: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(k).subtitle(v).build();
    let lbl = gtk::Label::new(Some(v));
    lbl.add_css_class("dim-more");
    lbl.add_css_class("monospace");
    row.add_suffix(&lbl);
    row
}
