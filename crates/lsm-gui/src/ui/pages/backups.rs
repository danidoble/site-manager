//! Backups: list + create + restore.

use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::widgets::{self, clear_listbox, margin_all, scrolled};
use crate::ui::{worker_backups, AppCtx, Event};
use lsm_core::domain::BackupEntry;

pub struct BackupsPage {
    pub body: gtk::Widget,
    pub actions: Vec<gtk::Widget>,
    pub list: gtk::ListBox,
}

impl BackupsPage {
    pub fn build(ctx: &AppCtx) -> Self {
        let create = gtk::Button::with_label("Create backup");
        create.add_css_class("suggested-action");
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Refresh"));

        {
            let ctx = ctx.clone();
            create.connect_clicked(move |_| {
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let r = lsm_core::App::new().and_then(|a| a.backup_create());
                    let _ = ctx.sender.send(match r {
                        Ok(_) => Event::Toast("Backup created".into()),
                        Err(e) => Event::Error(e.to_string()),
                    });
                    let _ = ctx.sender.send(worker_backups());
                });
            });
        }
        {
            let ctx = ctx.clone();
            refresh.connect_clicked(move |_| ctx.spawn(worker_backups));
        }

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        margin_all(&list, 12);
        root.append(&scrolled(&list));

        Self {
            body: root.upcast(),
            actions: vec![create.upcast(), refresh.upcast()],
            list,
        }
    }

    pub fn set_backups(&self, items: &[BackupEntry], ctx: &AppCtx) {
        clear_listbox(&self.list);
        if items.is_empty() {
            let empty = widgets::empty_state(
                "No backups",
                "Click “Create backup” to snapshot nginx configs, SSL metadata and app config.",
                "drive-harddisk-symbolic",
            );
            self.list
                .set_placeholder(Some(empty.upcast_ref::<gtk::Widget>()));
            return;
        }

        self.list.append(&header_row(&["Backup", "Created", "Size", "Actions"]));
        for b in items {
            self.list.append(&row(b, ctx));
        }
    }
}

fn row(b: &BackupEntry, ctx: &AppCtx) -> gtk::Widget {
    let r = gtk::ListBoxRow::new();
    r.set_selectable(false);
    let grid = gtk::Grid::new();
    grid.set_column_spacing(18);
    grid.set_margin_top(10);
    grid.set_margin_bottom(10);
    grid.set_margin_start(14);
    grid.set_margin_end(10);
    grid.attach(&text_cell(&b.name, true), 0, 0, 1, 1);
    grid.attach(&text_cell(&friendly_date(&b.created_at), true), 1, 0, 1, 1);
    grid.attach(&text_cell(&friendly_size(b.size_bytes), false), 2, 0, 1, 1);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let restore = gtk::Button::with_label("Restore");
    restore.add_css_class("flat");
    {
        let ctx = ctx.clone();
        let name = b.name.clone();
        restore.connect_clicked(move |_| {
            let ctx_resp = ctx.clone();
            let name_resp = name.clone();
            let alert = adw::AlertDialog::new(
                Some("Restore backup?"),
                Some(&format!("This restores config from “{name}”. Current files may be overwritten.")),
            );
            alert.add_response("cancel", "Cancel");
            alert.add_response("restore", "Restore");
            alert.set_response_appearance("restore", adw::ResponseAppearance::Destructive);
            alert.set_close_response("cancel");
            alert.connect_response(None, move |_d, resp: &str| {
                if resp == "restore" {
                    let ctx = ctx_resp.clone();
                    let name = name_resp.clone();
                    std::thread::spawn(move || {
                        let r = lsm_core::App::new().and_then(|a| a.backup_restore(&name));
                        let _ = ctx.sender.send(match r {
                            Ok(_) => Event::Toast("Backup restored".into()),
                            Err(e) => Event::Error(e.to_string()),
                        });
                    });
                }
            });
            alert.present(Some(&ctx.window));
        });
    }
    actions.append(&restore);

    let delete = gtk::Button::from_icon_name("user-trash-symbolic");
    delete.add_css_class("flat");
    delete.set_tooltip_text(Some("Delete backup"));
    {
        let ctx = ctx.clone();
        let name = b.name.clone();
        delete.connect_clicked(move |_| {
            let ctx_resp = ctx.clone();
            let name_resp = name.clone();
            let alert = adw::AlertDialog::new(
                Some("Delete backup?"),
                Some(&format!("This permanently deletes “{name}”.")),
            );
            alert.add_response("cancel", "Cancel");
            alert.add_response("delete", "Delete");
            alert.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            alert.set_close_response("cancel");
            alert.connect_response(None, move |_d, resp: &str| {
                if resp == "delete" {
                    let ctx = ctx_resp.clone();
                    let name = name_resp.clone();
                    std::thread::spawn(move || {
                        let r = lsm_core::App::new().and_then(|a| a.backup_delete(&name));
                        let _ = ctx.sender.send(match r {
                            Ok(_) => Event::Toast("Backup deleted".into()),
                            Err(e) => Event::Error(e.to_string()),
                        });
                        let _ = ctx.sender.send(worker_backups());
                    });
                }
            });
            alert.present(Some(&ctx.window));
        });
    }
    actions.append(&delete);
    grid.attach(&actions, 3, 0, 1, 1);
    r.set_child(Some(&grid));
    r.upcast()
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
        let l = text_cell(c, i < 2);
        l.add_css_class("dim-more");
        grid.attach(&l, i as i32, 0, 1, 1);
    }
    row.set_child(Some(&grid));
    row.upcast()
}

fn friendly_date(s: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    } else if s.len() == 16 && s.ends_with('Z') {
        s.to_string()
    } else {
        s.get(..19).unwrap_or(s).to_string()
    }
}

fn friendly_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn text_cell(text: &str, expand: bool) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_halign(gtk::Align::Fill);
    l.set_hexpand(expand);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l
}
