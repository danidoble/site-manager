//! DNS setup: dnsmasq + systemd-resolved + /etc/hosts guides.

use gtk::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;

use crate::ui::widgets::margin_all;
use crate::ui::AppCtx;
use crate::ui::Event;

pub struct DnsPage {
    pub body: gtk::Widget,
    pub actions: Vec<gtk::Widget>,
    dnsmasq_lbl: gtk::Label,
    hosts_lbl: gtk::Label,
    wild_lbl: gtk::Label,
}

impl DnsPage {
    pub fn build(ctx: &AppCtx) -> Self {
        let apply = gtk::Button::with_label("Apply DNS");

        let tld_e = gtk::Entry::new();
        tld_e.set_text("test");
        tld_e.set_width_chars(12);

        {
            let ctx = ctx.clone();
            let tld_e = tld_e.clone();
            apply.connect_clicked(move |_| {
                let tld = tld_e.text().to_string();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let r = lsm_core::App::new().and_then(|a| a.apply_dnsmasq(&tld));
                    let _ = ctx.sender.send(match r {
                        Ok(p) if p.success => Event::Toast(p.message),
                        Ok(p) => Event::Error(p.message),
                        Err(e) => Event::Error(e.to_string()),
                    });
                });
            });
        }

        let dnsmasq_lbl = mono_label();
        let hosts_lbl = mono_label();
        let wild_lbl = mono_label();

        let inner = gtk::Box::new(gtk::Orientation::Vertical, 18);
        margin_all(&inner, 12);

        // TLD row
        let tld_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        tld_box.append(&gtk::Label::new(Some("TLD:")));
        tld_box.append(&tld_e);

        inner.append(&section(
            "Using dnsmasq + systemd-resolved (recommended)",
            "Applies idempotent drop-ins for dnsmasq and systemd-resolved.",
            Some(&tld_box.upcast::<gtk::Widget>()),
            &dnsmasq_lbl,
        ));
        inner.append(&section(
            "Using /etc/hosts (manual)",
            "Static entries — one line per host.",
            None,
            &hosts_lbl,
        ));
        inner.append(&section(
            "Wildcards",
            "How wildcard *.domain resolution works.",
            None,
            &wild_lbl,
        ));

        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(820);
        clamp.set_child(Some(&inner));

        let scroll = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let sc = crate::ui::widgets::scrolled(&clamp);
        scroll.append(&sc);

        // Reload guides when TLD changes.
        {
            let ctx = ctx.clone();
            tld_e.connect_changed(move |e| {
                let tld = e.text().to_string();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let guides = lsm_core::App::new().map(|a| a.dns_guides(&tld));
                    let _ = ctx.sender.send(match guides {
                        Ok((d, h, w)) => Event::DnsGuides {
                            dnsmasq: d,
                            hosts: h,
                            wildcards: w,
                        },
                        Err(e) => Event::Error(e.to_string()),
                    });
                });
            });
        }

        Self {
            body: scroll.upcast(),
            actions: vec![apply.upcast()],
            dnsmasq_lbl,
            hosts_lbl,
            wild_lbl,
        }
    }

    pub fn set_guides(&self, dnsmasq: &str, hosts: &str, wildcards: &str) {
        self.dnsmasq_lbl.set_text(&plain_guide(dnsmasq));
        self.hosts_lbl.set_text(&plain_guide(hosts));
        self.wild_lbl.set_text(&plain_guide(wildcards));
    }
}

fn plain_guide(markdown: &str) -> String {
    markdown
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed == "```" || trimmed == "```sh" {
                None
            } else if let Some(title) = trimmed.strip_prefix("# ") {
                Some(title.to_string())
            } else if let Some(item) = trimmed.strip_prefix("- **") {
                Some(format!("- {}", item.replace("**: ", ": ")))
            } else {
                Some(line.trim_start().to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn mono_label() -> gtk::Label {
    let l = gtk::Label::new(Some(""));
    l.add_css_class("monospace");
    l.set_xalign(0.0);
    l.set_halign(gtk::Align::Start);
    l.set_wrap(true);
    l.set_selectable(true);
    l.add_css_class("code-block");
    l
}

fn section(title: &str, desc: &str, extra: Option<&gtk::Widget>, code: &gtk::Label) -> gtk::Widget {
    let v = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let t = gtk::Label::new(Some(title));
    t.set_halign(gtk::Align::Start);
    t.add_css_class("heading");
    let d = gtk::Label::new(Some(desc));
    d.set_halign(gtk::Align::Start);
    d.add_css_class("dim-more");
    v.append(&t);
    v.append(&d);
    if let Some(e) = extra {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.append(e);
        v.append(&row);
    }
    v.append(code);
    v.upcast()
}
