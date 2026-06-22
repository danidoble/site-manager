//! SSL / Certificates overview: CA status + cert list with renewal.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;

use crate::ui::widgets::{self, clear_listbox, margin_all, scrolled, Kind};
use crate::ui::{worker_certs, worker_ca_info, AppCtx, Event};
use lsm_core::domain::{Ca, SslCertificate};

pub struct SslPage {
    pub body: gtk::Widget,
    pub actions: Vec<gtk::Widget>,
    pub list: gtk::ListBox,
    ca_status: gtk::Label,
    shared: Rc<RefCell<Vec<SslCertificate>>>,
}

impl SslPage {
    pub fn build(ctx: &AppCtx) -> Self {
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Refresh"));
        let init_ca = gtk::Button::with_label("Initialize CA");
        let install_ca = gtk::Button::with_label("Install CA…");

        {
            let ctx = ctx.clone();
            refresh.connect_clicked(move |_| {
                ctx.spawn(worker_certs);
                ctx.spawn(worker_ca_info);
            });
        }
        {
            let ctx = ctx.clone();
            init_ca.connect_clicked(move |_| {
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let r = lsm_core::App::new().and_then(|a| a.init_ca());
                    let _ = ctx.sender.send(match r {
                        Ok(_) => Event::Toast("CA initialized".into()),
                        Err(e) => Event::Error(e.to_string()),
                    });
                    let _ = ctx.sender.send(worker_ca_info());
                });
            });
        }
        {
            let ctx = ctx.clone();
            install_ca.connect_clicked(move |_| {
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let r = lsm_core::App::new().and_then(|a| a.install_ca(None));
                    let _ = ctx.sender.send(match r {
                        Ok(p) if p.success => Event::Toast(p.message),
                        Ok(p) => Event::Error(p.message),
                        Err(e) => Event::Error(e.to_string()),
                    });
                });
            });
        }

        // CA banner.
        let ca_card = gtk::Box::new(gtk::Orientation::Vertical, 6);
        ca_card.add_css_class("stat-card");
        let ca_title = gtk::Label::new(Some("Local Root CA"));
        ca_title.add_css_class("heading");
        ca_title.set_halign(gtk::Align::Start);
        let ca_sub = gtk::Label::new(Some("30-year validity · trusted by browsers after install"));
        ca_sub.add_css_class("dim-more");
        ca_sub.set_halign(gtk::Align::Start);
        let ca_status = gtk::Label::new(Some("Loading…"));
        ca_status.set_halign(gtk::Align::Start);
        ca_status.set_xalign(0.0);
        ca_card.append(&ca_title);
        ca_card.append(&ca_sub);
        ca_card.append(&ca_status);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");

        let root = gtk::Box::new(gtk::Orientation::Vertical, 14);
        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(900);
        let inner = gtk::Box::new(gtk::Orientation::Vertical, 14);
        margin_all(&inner, 12);
        inner.append(&ca_card);
        clamp.set_child(Some(&inner));

        root.append(&clamp);
        root.append(&scrolled(&list));

        Self {
            body: root.upcast(),
            actions: vec![install_ca.upcast(), init_ca.upcast(), refresh.upcast()],
            list,
            ca_status,
            shared: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn set_ca(&self, ca: Option<&Ca>) {
        let text = match ca {
            Some(c) => format!(
                "Provider: {} · {} · fingerprint {}",
                c.provider,
                short_date(&c.created_at),
                &c.fingerprint[..12.min(c.fingerprint.len())]
            ),
            None => "No CA initialized yet. Click “Initialize CA”.".to_string(),
        };
        self.ca_status.set_text(&text);
    }

    pub fn set_certs(&self, certs: Vec<SslCertificate>, ctx: &AppCtx) {
        clear_listbox(&self.list);
        self.shared.borrow_mut().clear();
        self.shared.borrow_mut().extend(certs.iter().cloned());

        if certs.is_empty() {
            let empty = widgets::empty_state(
                "No certificates",
                "Certificates appear here once you configure a site with SSL.",
                "system-lock-screen-symbolic",
            );
            self.list
                .set_placeholder(Some(empty.upcast_ref::<gtk::Widget>()));
            return;
        }

        let mut sorted = certs.clone();
        sorted.sort_by(|a, b| a.not_after.cmp(&b.not_after));
        self.list.append(&header_row(&["Domain", "Issued", "Expires", "Status", "Actions"]));
        for c in sorted {
            self.list.append(&cert_row(&c, ctx));
        }
    }
}

fn cert_row(c: &SslCertificate, ctx: &AppCtx) -> gtk::Widget {
    let (kind, label) = expiry_state(&c.not_after);
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    let grid = gtk::Grid::new();
    grid.set_column_spacing(18);
    grid.set_margin_top(10);
    grid.set_margin_bottom(10);
    grid.set_margin_start(14);
    grid.set_margin_end(10);
    grid.attach(&text_cell(&c.domains.join(", "), true), 0, 0, 1, 1);
    grid.attach(&text_cell(short_date(&c.created_at), false), 1, 0, 1, 1);
    grid.attach(&text_cell(short_date(&c.not_after), false), 2, 0, 1, 1);
    grid.attach(&widgets::pill(kind, label), 3, 0, 1, 1);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let renew = gtk::Button::with_label("Renew");
    renew.add_css_class("flat");
    {
        let ctx = ctx.clone();
        let id = c.id;
        renew.connect_clicked(move |_| {
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r = lsm_core::App::new().and_then(|a| a.renew_cert(id));
                let _ = ctx.sender.send(match r {
                    Ok(_) => Event::Toast("Certificate renewed".into()),
                    Err(e) => Event::Error(e.to_string()),
                });
                let _ = ctx.sender.send(worker_certs());
            });
        });
    }
    actions.append(&renew);
    let delete = gtk::Button::from_icon_name("user-trash-symbolic");
    delete.add_css_class("flat");
    delete.set_tooltip_text(Some("Delete certificate"));
    {
        let ctx = ctx.clone();
        let id = c.id;
        delete.connect_clicked(move |_| {
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r = lsm_core::App::new().and_then(|a| a.delete_cert(id));
                let _ = ctx.sender.send(match r {
                    Ok(_) => Event::Toast("Certificate deleted".into()),
                    Err(e) => Event::Error(e.to_string()),
                });
                let _ = ctx.sender.send(worker_certs());
                let _ = ctx.sender.send(crate::ui::worker_sites());
            });
        });
    }
    actions.append(&delete);
    grid.attach(&actions, 4, 0, 1, 1);
    row.set_child(Some(&grid));
    row.upcast()
}

fn header_row(cols: &[&str]) -> gtk::Widget {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);
    let grid = gtk::Grid::new();
    grid.set_column_spacing(18);
    grid.set_margin_top(8);
    grid.set_margin_bottom(8);
    grid.set_margin_start(14);
    grid.set_margin_end(10);
    for (i, c) in cols.iter().enumerate() {
        let l = text_cell(c, i == 0);
        l.add_css_class("dim-more");
        grid.attach(&l, i as i32, 0, 1, 1);
    }
    row.set_child(Some(&grid));
    row.upcast()
}

fn text_cell(text: &str, expand: bool) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_halign(gtk::Align::Fill);
    l.set_hexpand(expand);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l
}

/// Return (kind, label) from an RFC3339 expiry. Lexical compare works for the
/// fixed-format timestamps emitted by the core.
fn expiry_state(not_after: &str) -> (Kind, &'static str) {
    // Approximate "expiring soon" with a 30-day horizon using a coarse string
    // probe against today's date prefix.
    let today = now_prefix();
    if not_after < today.as_str() {
        (Kind::Error, "expired")
    } else if starts_within_30d(not_after, &today) {
        (Kind::Warning, "expiring soon")
    } else {
        (Kind::Success, "valid")
    }
}

fn now_prefix() -> String {
    // Defer to chrono when available; fall back to an empty-ish probe.
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn starts_within_30d(not_after: &str, today: &str) -> bool {
    let n = chrono::DateTime::parse_from_rfc3339(not_after).ok();
    let t = chrono::DateTime::parse_from_rfc3339(&format!("{today}Z")).ok();
    match (n, t) {
        (Some(n), Some(t)) => {
            (n.with_timezone(&chrono::Utc) - t.with_timezone(&chrono::Utc)).num_days() < 30
        }
        _ => false,
    }
}

fn short_date(iso: &str) -> &str {
    iso.get(..10).unwrap_or(iso)
}
