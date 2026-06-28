//! Health checks for proxy sites.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::ui::widgets::{self, clear_listbox, margin_all, scrolled};
use crate::ui::{worker_health, AppCtx, Event};
use lsm_core::domain::{HealthCheck, Site};

pub struct HealthPage {
    pub body: gtk::Widget,
    pub actions: Vec<gtk::Widget>,
    pub list: gtk::ListBox,
}

impl HealthPage {
    pub fn build(ctx: &AppCtx) -> Self {
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Refresh"));
        let check = gtk::Button::with_label("Check all");

        {
            let ctx = ctx.clone();
            refresh.connect_clicked(move |_| ctx.spawn(worker_health));
        }
        {
            let ctx = ctx.clone();
            check.connect_clicked(move |_| {
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    match lsm_core::App::new().and_then(|app| {
                        let sites = app.list_sites(None, 1, 500)?;
                        let mut checked = 0usize;
                        for s in sites
                            .iter()
                            .filter(|s| s.site_type == lsm_core::domain::SiteType::Proxy)
                        {
                            app.check_proxy(s.id)?;
                            checked += 1;
                        }
                        Ok(checked)
                    }) {
                        Ok(checked) => {
                            let msg = if checked == 0 {
                                "No proxy sites to check".to_string()
                            } else {
                                format!(
                                    "Checked {checked} proxy site{}",
                                    if checked == 1 { "" } else { "s" }
                                )
                            };
                            let _ = ctx.sender.send(Event::Toast(msg));
                            let _ = ctx.sender.send(worker_health());
                        }
                        Err(e) => {
                            let _ = ctx.sender.send(Event::Error(e.to_string()));
                            let _ = ctx.sender.send(worker_health());
                        }
                    }
                });
            });
        }

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        margin_all(&list, 12);
        root.append(&scrolled(&list));

        Self {
            body: root.upcast(),
            actions: vec![check.upcast(), refresh.upcast()],
            list,
        }
    }

    pub fn set_health(&self, items: &[(Site, Option<HealthCheck>)]) {
        clear_listbox(&self.list);
        let proxies: Vec<&(Site, Option<HealthCheck>)> = items
            .iter()
            .filter(|(s, _)| s.site_type == lsm_core::domain::SiteType::Proxy)
            .collect();

        if proxies.is_empty() {
            let empty = widgets::empty_state(
                "No proxy sites",
                "Add a reverse-proxy site to monitor its health.",
                "network-transmit-receive-symbolic",
            );
            self.list
                .set_placeholder(Some(empty.upcast_ref::<gtk::Widget>()));
            return;
        }

        self.list
            .append(&header_row(&["Domain", "Target", "Last check", "Status"]));
        for (site, h) in proxies {
            self.list.append(&row(site, h));
        }
    }
}

fn row(site: &Site, h: &Option<HealthCheck>) -> gtk::Widget {
    let (icon, label, sub) = match h {
        Some(h) if h.healthy => (
            "emblem-ok-symbolic",
            format!("Healthy · {} ms", h.response_ms.unwrap_or(0)),
            format!("last check {}", short(&h.checked_at)),
        ),
        Some(h) => (
            "dialog-error-symbolic",
            "Unhealthy".to_string(),
            h.error
                .clone()
                .unwrap_or_else(|| short(&h.checked_at).to_string()),
        ),
        None => (
            "dialog-information-symbolic",
            "Not checked".to_string(),
            "click Check all".to_string(),
        ),
    };

    let r = gtk::ListBoxRow::new();
    r.set_selectable(false);
    let grid = gtk::Grid::new();
    grid.set_column_spacing(18);
    grid.set_margin_top(10);
    grid.set_margin_bottom(10);
    grid.set_margin_start(14);
    grid.set_margin_end(10);
    grid.attach(&text_cell(&site.primary_domain, true), 0, 0, 1, 1);
    grid.attach(
        &text_cell(&site.proxy_target.clone().unwrap_or_default(), true),
        1,
        0,
        1,
        1,
    );
    grid.attach(&text_cell(&sub, true), 2, 0, 1, 1);
    grid.attach(&status_cell(icon, &label), 3, 0, 1, 1);
    r.set_child(Some(&grid));
    r.upcast()
}

fn status_cell(icon: &str, text: &str) -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    box_.append(&gtk::Image::from_icon_name(icon));
    let label = text_cell(text, false);
    label.add_css_class("dim-label");
    box_.append(&label);
    box_
}

fn short(s: &str) -> &str {
    s.get(..19).unwrap_or(s)
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
        let l = text_cell(c, i < 3);
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
