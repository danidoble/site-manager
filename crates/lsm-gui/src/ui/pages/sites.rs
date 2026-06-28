//! Sites list: searchable, with per-row actions + detail dialog.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4 as gtk;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::widgets::{clear_listbox, empty_state, margin_all, scrolled};
use crate::ui::{worker_sites, AppCtx, Event};
use lsm_core::domain::{Site, SiteType};

type SharedSites = Rc<RefCell<Vec<Site>>>;
const COL_DOMAIN: i32 = 240;
const COL_MODE: i32 = 150;
const COL_SSL: i32 = 90;
const COL_ACTIONS: i32 = 96;

pub struct SitesPage {
    pub body: gtk::Widget,
    pub actions: Vec<gtk::Widget>,
    pub list: gtk::ListBox,
    pub search: gtk::SearchEntry,
    shared: SharedSites,
}

impl SitesPage {
    pub fn build(ctx: &AppCtx) -> Self {
        let new_btn = gtk::Button::from_icon_name("list-add-symbolic");
        new_btn.set_tooltip_text(Some("New site"));
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Refresh"));

        {
            let ctx = ctx.clone();
            new_btn.connect_clicked(move |_| super::new_site::open(&ctx, None));
        }
        {
            let ctx = ctx.clone();
            refresh.connect_clicked(move |_| ctx.spawn(worker_sites));
        }

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Search sites…"));

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");

        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        margin_all(&search, 2);
        root.append(&search);
        root.append(&scrolled(&list));

        let shared: SharedSites = Rc::new(RefCell::new(Vec::new()));

        // Re-render on each keystroke.
        {
            let list = list.clone();
            let shared = Rc::clone(&shared);
            let ctx = ctx.clone();
            search.connect_search_changed(move |entry| {
                render(&list, &shared, &entry.text(), &ctx);
            });
        }

        Self {
            body: root.upcast(),
            actions: vec![new_btn.upcast(), refresh.upcast()],
            list,
            search,
            shared,
        }
    }

    pub fn set_sites(&self, sites: Vec<Site>, ctx: &AppCtx) {
        self.shared.borrow_mut().clear();
        self.shared.borrow_mut().extend(sites);
        render(&self.list, &self.shared, &self.search.text(), ctx);
    }
}

fn render(list: &gtk::ListBox, shared: &SharedSites, needle: &str, ctx: &AppCtx) {
    clear_listbox(list);
    let sites = shared.borrow();
    let needle = needle.to_lowercase();
    let filtered: Vec<&Site> = sites
        .iter()
        .filter(|s| {
            needle.is_empty()
                || s.name.to_lowercase().contains(&needle)
                || s.primary_domain.to_lowercase().contains(&needle)
        })
        .collect();

    if filtered.is_empty() {
        let empty: gtk::Widget = if sites.is_empty() {
            first_site_empty(ctx)
        } else {
            empty_state(
                "No matches",
                "No sites match your search.",
                "system-search-symbolic",
            )
            .upcast()
        };
        list.set_placeholder(Some(&empty));
        return;
    }

    list.append(&sites_header());
    for s in filtered {
        list.append(&site_row(s, ctx));
    }
}

fn first_site_empty(ctx: &AppCtx) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 14);
    box_.set_valign(gtk::Align::Center);
    box_.set_halign(gtk::Align::Center);
    box_.set_vexpand(true);
    box_.set_margin_top(48);
    box_.set_margin_bottom(48);

    let icon = gtk::Image::from_icon_name("list-add-symbolic");
    icon.set_pixel_size(72);
    let title = gtk::Label::new(Some("No sites yet"));
    title.add_css_class("title-1");
    let desc = gtk::Label::new(Some(
        "Create your first local domain and configure nginx in one flow.",
    ));
    desc.add_css_class("dim-more");
    desc.set_wrap(true);
    desc.set_justify(gtk::Justification::Center);
    let button = gtk::Button::with_label("Create first site");
    button.add_css_class("suggested-action");
    {
        let ctx = ctx.clone();
        button.connect_clicked(move |_| super::new_site::open(&ctx, None));
    }

    box_.append(&icon);
    box_.append(&title);
    box_.append(&desc);
    box_.append(&button);
    box_.upcast()
}

fn site_row(site: &Site, ctx: &AppCtx) -> gtk::Widget {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(true);
    let grid = gtk::Grid::new();
    grid.set_column_spacing(18);
    grid.set_margin_top(10);
    grid.set_margin_bottom(10);
    grid.set_margin_start(14);
    grid.set_margin_end(10);
    grid.attach(&domain_cell(site), 0, 0, 1, 1);
    grid.attach(&type_label(site), 1, 0, 1, 1);
    grid.attach(&ssl_label(site), 2, 0, 1, 1);
    grid.attach(&label_cell(&site.project_path, 0.0, true), 3, 0, 1, 1);
    grid.attach(&row_actions(site, ctx), 4, 0, 1, 1);
    for (col, width, expand) in [
        (0, COL_DOMAIN, false),
        (1, COL_MODE, false),
        (2, COL_SSL, false),
        (3, -1, true),
        (4, COL_ACTIONS, false),
    ] {
        if let Some(child) = grid.child_at(col, 0) {
            child.set_size_request(width, -1);
            child.set_hexpand(expand);
        }
    }
    let id = site.id;
    let ctx = ctx.clone();
    row.connect_activate(move |_| {
        open_detail(&ctx, id);
    });
    row.set_child(Some(&grid));
    row.upcast()
}

fn sites_header() -> gtk::Widget {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);
    let grid = gtk::Grid::new();
    grid.set_column_spacing(18);
    grid.set_margin_top(8);
    grid.set_margin_bottom(8);
    grid.set_margin_start(14);
    grid.set_margin_end(10);
    for (idx, title) in ["Domain", "Mode", "SSL", "Path", "Actions"]
        .iter()
        .enumerate()
    {
        let l = label_cell(title, 0.0, idx == 3);
        l.add_css_class("dim-more");
        grid.attach(&l, idx as i32, 0, 1, 1);
    }
    for (col, width, expand) in [
        (0, COL_DOMAIN, false),
        (1, COL_MODE, false),
        (2, COL_SSL, false),
        (3, -1, true),
        (4, COL_ACTIONS, false),
    ] {
        if let Some(child) = grid.child_at(col, 0) {
            child.set_size_request(width, -1);
            child.set_hexpand(expand);
        }
    }
    row.set_child(Some(&grid));
    row.upcast()
}

fn domain_cell(site: &Site) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
    b.set_size_request(240, -1);
    let title = label_cell(&site.primary_domain, 0.0, false);
    title.add_css_class("heading");
    let sub = if site.aliases.is_empty() {
        "No aliases".to_string()
    } else {
        site.aliases.join(", ")
    };
    let aliases = label_cell(&sub, 0.0, false);
    aliases.add_css_class("dim-more");
    b.append(&title);
    b.append(&aliases);
    b
}

fn label_cell(text: &str, xalign: f32, expand: bool) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(xalign);
    l.set_halign(gtk::Align::Fill);
    l.set_hexpand(expand);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l
}

fn type_label(s: &Site) -> gtk::Label {
    let text = match s.site_type {
        SiteType::Php => format!("PHP {}", s.php_version.clone().unwrap_or_default()),
        SiteType::Proxy => format!("Proxy {}", s.proxy_target.clone().unwrap_or_default()),
        SiteType::Static => "Static".to_string(),
    };
    let label = label_cell(&text, 0.0, false);
    label.add_css_class("dim-more");
    label
}

fn ssl_label(s: &Site) -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let icon_name = if s.ssl_cert_id.is_some() {
        "channel-secure-symbolic"
    } else {
        "channel-insecure-symbolic"
    };
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    let text = if s.ssl_cert_id.is_some() {
        "SSL"
    } else {
        "No SSL"
    };
    let label = label_cell(text, 0.0, false);
    label.add_css_class("dim-more");
    box_.append(&icon);
    box_.append(&label);
    box_
}

fn row_actions(site: &Site, ctx: &AppCtx) -> gtk::Box {
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    actions.set_halign(gtk::Align::End);
    let open = gtk::Button::from_icon_name("document-open-symbolic");
    open.add_css_class("flat");
    open.set_tooltip_text(Some("Open site"));
    {
        let domain = site.primary_domain.trim_start_matches("*.").to_string();
        open.connect_clicked(move |_| {
            let _ = lsm_core::App::open_in_browser(&format!("https://{domain}/"));
        });
    }
    actions.append(&open);
    actions.append(&actions_menu(site, ctx));
    actions
}

fn actions_menu(site: &Site, ctx: &AppCtx) -> gtk::MenuButton {
    let btn = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("Actions")
        .build();
    btn.add_css_class("flat");
    let popover = gtk::Popover::new();
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
    box_.set_margin_top(6);
    box_.set_margin_bottom(6);
    box_.set_margin_start(6);
    box_.set_margin_end(6);
    let id = site.id;
    let domain = site.primary_domain.trim_start_matches("*.").to_string();
    add_menu_button(&box_, "Edit", {
        let ctx = ctx.clone();
        move || open_detail(&ctx, id)
    });
    add_menu_button(&box_, "Open", move || {
        let _ = lsm_core::App::open_in_browser(&format!("https://{domain}/"));
    });
    add_menu_button(&box_, "Apply SSL", {
        let ctx = ctx.clone();
        move || {
            run_site_job(&ctx, id, "SSL applied", |a, id| {
                a.configure_site(id, true).map(|_| ())
            })
        }
    });
    add_menu_button(&box_, "Renew SSL", {
        let ctx = ctx.clone();
        move || {
            run_site_job(&ctx, id, "SSL renewed", |a, id| {
                a.configure_site(id, true).map(|_| ())
            })
        }
    });
    add_menu_button(&box_, "Delete", {
        let ctx = ctx.clone();
        move || confirm_delete(&ctx, None, id)
    });
    popover.set_child(Some(&box_));
    btn.set_popover(Some(&popover));
    btn
}

fn add_menu_button<F: Fn() + 'static>(box_: &gtk::Box, label: &str, f: F) {
    let btn = gtk::Button::new();
    btn.add_css_class("flat");
    btn.set_halign(gtk::Align::Fill);
    let l = gtk::Label::new(Some(label));
    l.set_xalign(0.0);
    l.set_halign(gtk::Align::Fill);
    btn.set_child(Some(&l));
    btn.connect_clicked(move |_| f());
    box_.append(&btn);
}

fn run_site_job<F>(ctx: &AppCtx, site_id: i64, success: &'static str, f: F)
where
    F: FnOnce(&lsm_core::App, i64) -> lsm_core::Result<()> + Send + 'static,
{
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let r = lsm_core::App::new().and_then(|a| f(&a, site_id));
        let _ = ctx.sender.send(match r {
            Ok(_) => Event::Toast(success.into()),
            Err(e) => Event::Error(e.to_string()),
        });
        let _ = ctx.sender.send(worker_sites());
        let _ = ctx.sender.send(crate::ui::worker_certs());
    });
}

/// Open a detail/management window for a site.
pub fn open_detail(ctx: &AppCtx, site_id: i64) {
    tracing::info!(site_id, "opening site detail dialog");
    let dialog = adw::Dialog::builder()
        .title("Site")
        .content_width(560)
        .content_height(640)
        .build();

    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    let save_btn = gtk::Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
    close_btn.add_css_class("flat");
    close_btn.set_tooltip_text(Some("Close"));
    {
        let dialog = dialog.clone();
        close_btn.connect_clicked(move |_| {
            let _ = dialog.close();
        });
    }
    header.pack_start(&close_btn);
    header.pack_end(&save_btn);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    margin_all(&content, 14);

    let site = match lsm_core::App::new().and_then(|a| a.get_site(site_id)) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(site_id, error = %e, "could not load site detail");
            ctx.toast(&format!("Could not load site: {e}"));
            return;
        }
    };

    let title = adw::WindowTitle::new(&site.name, &site.primary_domain);
    header.set_title_widget(Some(&title));

    let info = adw::PreferencesGroup::new();
    info.set_title("Edit");
    let backend = match site.site_type {
        SiteType::Php => site.php_version.clone().unwrap_or_default(),
        SiteType::Proxy => site.proxy_target.clone().unwrap_or_default(),
        SiteType::Static => "—".into(),
    };
    let domain_row = adw::EntryRow::builder()
        .title("Domain")
        .text(&site.primary_domain)
        .build();
    let path_row = adw::EntryRow::builder()
        .title("Project root")
        .text(&site.project_path)
        .build();
    let backend_title = match site.site_type {
        SiteType::Php => "PHP version",
        SiteType::Proxy => "Proxy target",
        SiteType::Static => "Backend",
    };
    let backend_row = adw::EntryRow::builder()
        .title(backend_title)
        .text(&backend)
        .build();
    backend_row.set_sensitive(site.site_type != SiteType::Static);
    let wildcard_cb = gtk::CheckButton::with_label("Wildcard *.domain");
    wildcard_cb.set_active(site.wildcard);
    let wildcard_row = adw::ActionRow::builder().title("Wildcard").build();
    wildcard_row.add_suffix(&wildcard_cb);
    let websocket_cb = gtk::CheckButton::with_label("Enable WebSockets");
    websocket_cb.set_active(site.websocket);
    let websocket_row = adw::ActionRow::builder()
        .title("WebSockets")
        .subtitle("Nginx Upgrade/Connection headers")
        .build();
    websocket_row.set_visible(site.site_type == SiteType::Proxy);
    websocket_row.add_suffix(&websocket_cb);
    let type_row = adw::ActionRow::builder()
        .title("Type")
        .subtitle(site.site_type.as_str())
        .build();
    let nginx_row = adw::ActionRow::builder()
        .title("Nginx config")
        .subtitle(format!("{}.conf", site.name))
        .build();
    info.add(&domain_row);
    info.add(&path_row);
    info.add(&backend_row);
    info.add(&wildcard_row);
    info.add(&websocket_row);
    info.add(&type_row);
    info.add(&nginx_row);
    content.append(&info);

    let open_btn = gtk::Button::with_label("Open");
    {
        let domain = site.primary_domain.trim_start_matches("*.").to_string();
        open_btn.connect_clicked(move |_| {
            let _ = lsm_core::App::open_in_browser(&format!("https://{domain}/"));
        });
    }

    let ssl_btn = gtk::Button::with_label("Regenerate SSL");
    {
        let ctx = ctx.clone();
        ssl_btn.connect_clicked(move |_| {
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r = lsm_core::App::new().and_then(|a| a.configure_site(site_id, true));
                let _ = ctx.sender.send(match r {
                    Ok(_) => Event::Toast("SSL certificate regenerated".into()),
                    Err(e) => Event::Error(e.to_string()),
                });
                let _ = ctx.sender.send(worker_sites());
                let _ = ctx.sender.send(crate::ui::worker_certs());
            });
        });
    }

    let configure_btn = gtk::Button::with_label("Configure");
    {
        let ctx = ctx.clone();
        configure_btn.connect_clicked(move |_| {
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r = lsm_core::App::new().and_then(|a| a.configure_site(site_id, true));
                let _ = ctx.sender.send(match r {
                    Ok(_) => Event::Toast("Site configured".into()),
                    Err(e) => Event::Error(e.to_string()),
                });
                let _ = ctx.sender.send(worker_sites());
                let _ = ctx.sender.send(crate::ui::worker_certs());
            });
        });
    }

    {
        let ctx = ctx.clone();
        let dialog = dialog.clone();
        let editable = site.clone();
        let domain_row = domain_row.clone();
        let path_row = path_row.clone();
        let backend_row = backend_row.clone();
        let wildcard_cb = wildcard_cb.clone();
        let websocket_cb = websocket_cb.clone();
        save_btn.connect_clicked(move |_| {
            let mut next = editable.clone();
            next.primary_domain = domain_row.text().trim().to_string();
            next.project_path = path_row.text().trim().to_string();
            next.wildcard = wildcard_cb.is_active();
            next.websocket = next.site_type == SiteType::Proxy && websocket_cb.is_active();
            match next.site_type {
                SiteType::Php => next.php_version = optional_text(&backend_row.text()),
                SiteType::Proxy => next.proxy_target = optional_text(&backend_row.text()),
                SiteType::Static => {}
            }
            let ctx = ctx.clone();
            let _ = dialog.close();
            std::thread::spawn(move || {
                let r = lsm_core::App::new().and_then(|a| {
                    let updated = a.update_site(next)?;
                    a.configure_site(updated.id, false)?;
                    Ok(updated)
                });
                if let Err(e) = &r {
                    tracing::error!(site_id, error = %e, "site update failed");
                }
                let _ = ctx.sender.send(match r {
                    Ok(_) => Event::Toast("Site updated".into()),
                    Err(e) => Event::Error(e.to_string()),
                });
                let _ = ctx.sender.send(worker_sites());
                let _ = ctx.sender.send(crate::ui::worker_certs());
            });
        });
    }

    let hosts_btn = gtk::Button::with_label("Add hosts");
    {
        let ctx = ctx.clone();
        hosts_btn.connect_clicked(move |_| {
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r = lsm_core::App::new().and_then(|a| {
                    let site = a.get_site(site_id)?;
                    a.add_hosts_for_site(&site)
                });
                let _ = ctx.sender.send(match r {
                    Ok(p) => Event::Toast(p.message),
                    Err(e) => Event::Error(e.to_string()),
                });
            });
        });
    }

    let remove_hosts_btn = gtk::Button::with_label("Remove hosts");
    {
        let ctx = ctx.clone();
        remove_hosts_btn.connect_clicked(move |_| {
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r = lsm_core::App::new().and_then(|a| {
                    let site = a.get_site(site_id)?;
                    a.remove_hosts_for_site(&site)
                });
                let _ = ctx.sender.send(match r {
                    Ok(p) => Event::Toast(p.message),
                    Err(e) => Event::Error(e.to_string()),
                });
            });
        });
    }

    let delete_btn = gtk::Button::with_label("Delete");
    delete_btn.add_css_class("destructive-action");
    {
        let ctx = ctx.clone();
        let dialog = dialog.clone();
        delete_btn.connect_clicked(move |_| {
            confirm_delete(&ctx, Some(&dialog), site_id);
        });
    }

    let actions = adw::PreferencesGroup::new();
    actions.set_title("Actions");
    actions.add(&button_row(
        "Open in browser",
        "Launch the site URL",
        &open_btn,
    ));
    actions.add(&button_row(
        "Configure nginx",
        "Write config, test and reload nginx",
        &configure_btn,
    ));
    actions.add(&button_row(
        "Regenerate SSL",
        "Issue a fresh certificate and reload nginx",
        &ssl_btn,
    ));
    content.append(&actions);

    let dns = adw::PreferencesGroup::new();
    dns.set_title("Hosts");
    dns.add(&button_row(
        "Add hosts entries",
        "Add managed /etc/hosts records",
        &hosts_btn,
    ));
    dns.add(&button_row(
        "Remove hosts entries",
        "Remove managed /etc/hosts records",
        &remove_hosts_btn,
    ));
    content.append(&dns);

    let danger = adw::PreferencesGroup::new();
    danger.set_title("Danger");
    danger.add(&button_row(
        "Delete site",
        "Remove nginx config, hosts entries and attached certificate",
        &delete_btn,
    ));
    content.append(&danger);

    content.set_size_request(-1, 560);
    let scroll = scrolled(&content);
    scroll.set_min_content_height(560);
    toolbar.set_content(Some(&scroll));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(&ctx.window));
}

fn button_row(title: &str, subtitle: &str, button: &gtk::Button) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    row.add_suffix(button);
    row
}

fn confirm_delete(ctx: &AppCtx, parent: Option<&adw::Dialog>, site_id: i64) {
    let name = lsm_core::App::new()
        .and_then(|a| a.get_site(site_id))
        .map(|s| s.name)
        .unwrap_or_else(|_| "site".into());
    let ctx_resp = ctx.clone();
    let parent_close = parent.cloned();
    let alert = adw::AlertDialog::new(
        Some("Delete site?"),
        Some(&format!(
            "This removes “{name}”, its nginx config and attached SSL certificate."
        )),
    );
    alert.add_response("cancel", "Cancel");
    alert.add_response("delete", "Delete");
    alert.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    alert.set_close_response("cancel");
    alert.connect_response(None, move |_d, resp: &str| {
        if resp == "delete" {
            let ctx = ctx_resp.clone();
            let parent_close = parent_close.clone();
            std::thread::spawn(move || {
                let r = lsm_core::App::new().and_then(|a| a.delete_site(site_id));
                let _ = ctx.sender.send(match r {
                    Ok(_) => Event::Toast("Site deleted".into()),
                    Err(e) => Event::Error(e.to_string()),
                });
                let _ = ctx.sender.send(worker_sites());
                let _ = ctx.sender.send(crate::ui::worker_certs());
            });
            if let Some(parent) = parent_close {
                let _ = parent.close();
            }
        }
    });
    alert.present(Some(&ctx.window));
}

fn optional_text(s: &glib::GString) -> Option<String> {
    let text = s.trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
