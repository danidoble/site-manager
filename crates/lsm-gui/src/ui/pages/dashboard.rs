//! Dashboard: at-a-glance stat cards + recent activity.

use gtk4 as gtk;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::widgets::{clear_box, clear_listbox, margin_all};
use crate::ui::{worker_diagnostics, worker_status, AppCtx, Event};
use lsm_core::domain::{Site, Status};

pub struct DashboardPage {
    pub body: gtk::Widget,
    pub actions: Vec<gtk::Widget>,
    cards: gtk::Box,
    recent: gtk::ListBox,
}

impl DashboardPage {
    pub fn build(ctx: &AppCtx) -> Self {
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Refresh status"));
        let diag = gtk::Button::from_icon_name("system-run-symbolic");
        diag.set_tooltip_text(Some("Run diagnostics"));

        {
            let ctx = ctx.clone();
            refresh.connect_clicked(move |_| ctx.spawn(worker_status));
        }
        {
            let ctx = ctx.clone();
            diag.connect_clicked(move |_| ctx.spawn(worker_diagnostics));
        }

        let root = gtk::Box::new(gtk::Orientation::Vertical, 18);
        margin_all(&root, 6);

        let cards = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let services = service_controls(ctx);

        let recent = gtk::ListBox::new();
        recent.set_selection_mode(gtk::SelectionMode::None);
        recent.add_css_class("boxed-list");
        let recent_group = adw::PreferencesGroup::new();
        recent_group.set_title("Recent activity");
        recent_group.set_description(Some("Latest local changes"));
        recent_group.add(&recent);

        let lower = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        lower.set_homogeneous(true);
        lower.append(&services);
        lower.append(&recent_group);

        root.append(&cards);
        root.append(&lower);

        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(900);
        clamp.set_child(Some(&root));
        let body = clamp.upcast::<gtk::Widget>();

        Self {
            body,
            actions: vec![diag.upcast(), refresh.upcast()],
            cards,
            recent,
        }
    }

    pub fn set_status(&self, st: &Status) {
        clear_box(&self.cards);

        let php = if st.php_versions.is_empty() {
            st.php_status.clone()
        } else {
            st.php_versions.join(", ")
        };

        let group = adw::PreferencesGroup::new();
        group.set_title("Overview");
        group.set_description(Some(&format!("PHP-FPM detected: {php}")));
        group.add(&overview_row(
            "Local sites",
            "Managed domains",
            "applications-internet-symbolic",
            &st.sites_count.to_string(),
        ));
        group.add(&overview_row(
            "SSL",
            "Local certificate authority",
            "channel-secure-symbolic",
            &st.ssl_status,
        ));
        group.add(&overview_row(
            "Nginx",
            st.nginx_layout.as_str(),
            "network-server-symbolic",
            &normalize_ok(&st.nginx_status),
        ));
        group.add(&overview_row(
            "DNS",
            "*.test resolver",
            "network-workgroup-symbolic",
            &normalize_ok(&st.dnsmasq_status),
        ));
        self.cards.append(&group);
    }

    pub fn set_recent(&self, sites: &[Site]) {
        clear_listbox(&self.recent);
        if sites.is_empty() {
            self.recent.append(&recent_empty_row());
            return;
        }
        let mut recent: Vec<&Site> = sites.iter().collect();
        recent.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        for s in recent.iter().take(6) {
            let row = adw::ActionRow::builder()
                .title(&s.name)
                .subtitle(&format!(
                    "{} · {}",
                    s.primary_domain,
                    short_date(&s.updated_at)
                ))
                .build();
            row.add_prefix(&site_type_icon(s));
            row.add_suffix(&status_text(if s.ssl_cert_id.is_some() {
                "SSL"
            } else {
                "No SSL"
            }));
            self.recent.append(&row);
        }
    }
}

fn overview_row(title: &str, subtitle: &str, icon: &str, value: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    row.add_prefix(&gtk::Image::from_icon_name(icon));
    row.add_suffix(&status_text(value));
    row
}

fn status_text(value: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.add_css_class("dim-label");
    label.set_xalign(1.0);
    label
}

fn recent_empty_row() -> gtk::Widget {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
    box_.set_margin_top(18);
    box_.set_margin_bottom(18);
    box_.set_margin_start(14);
    box_.set_margin_end(14);
    let title = gtk::Label::new(Some("No local activity yet"));
    title.add_css_class("heading");
    title.set_halign(gtk::Align::Start);
    let sub = gtk::Label::new(Some(
        "Create your first domain to start seeing recent changes here.",
    ));
    sub.add_css_class("dim-more");
    sub.set_halign(gtk::Align::Start);
    box_.append(&title);
    box_.append(&sub);
    row.set_child(Some(&box_));
    row.upcast()
}

fn normalize_ok(value: &str) -> String {
    if value == "installed" {
        "OK".into()
    } else {
        value.to_string()
    }
}

fn service_controls(ctx: &AppCtx) -> gtk::Widget {
    let group = adw::PreferencesGroup::new();
    group.set_title("Services");

    for (title, action, service) in [
        ("Reload nginx", "reload", "nginx"),
        ("Restart nginx", "restart", "nginx"),
        ("Restart PHP-FPM", "restart-php", "php-fpm"),
        ("Auto-renew timer", "is-enabled", "local-site-manager.timer"),
    ] {
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(service)
            .build();
        let btn = gtk::Button::from_icon_name(if action == "is-enabled" {
            "emblem-system-symbolic"
        } else {
            "view-refresh-symbolic"
        });
        btn.add_css_class("flat");
        {
            let ctx = ctx.clone();
            let action = action.to_string();
            let service = service.to_string();
            btn.connect_clicked(move |_| {
                let ctx = ctx.clone();
                let action = action.clone();
                let service = service.clone();
                std::thread::spawn(move || {
                    let result = lsm_core::App::new().and_then(|app| {
                        if action == "restart-php" {
                            let versions = lsm_core::diagnostics::detect_php_fpm_versions();
                            if versions.is_empty() {
                                return Err(lsm_core::Error::Other(
                                    "no php-fpm versions detected".into(),
                                ));
                            }
                            for version in versions {
                                app.systemctl("restart", &format!("php{version}-fpm"))?;
                            }
                            Ok("PHP-FPM restarted".to_string())
                        } else {
                            if action == "is-enabled" {
                                let res = app.systemctl_capture(&action, &service)?;
                                let state = if res.success {
                                    res.stdout.trim().to_string()
                                } else if res.stdout.trim().is_empty() {
                                    "disabled or not installed".to_string()
                                } else {
                                    res.stdout.trim().to_string()
                                };
                                return Ok(format!("Auto-renew timer: {state}"));
                            }
                            let res = app.systemctl(&action, &service)?;
                            Ok(res.message)
                        }
                    });
                    let _ = ctx.sender.send(match result {
                        Ok(msg) => Event::Toast(msg),
                        Err(e) => Event::Error(e.to_string()),
                    });
                    let _ = ctx.sender.send(worker_status());
                });
            });
        }
        row.add_suffix(&btn);
        group.add(&row);
    }

    group.upcast()
}

fn site_type_icon(s: &Site) -> gtk::Image {
    use lsm_core::domain::SiteType;
    let icon = match s.site_type {
        SiteType::Php => "application-x-php-symbolic",
        SiteType::Proxy => "network-transmit-receive-symbolic",
        SiteType::Static => "text-html-symbolic",
    };
    gtk::Image::from_icon_name(icon)
}

fn short_date(iso: &str) -> &str {
    iso.get(..10).unwrap_or(iso)
}
