//! Logs viewer: source dropdown + monospace tail.

use gtk4 as gtk;
use gtk::prelude::*;
use gtk::StringList;

use crate::ui::widgets::{margin_all, scrolled};
use crate::ui::AppCtx;
use crate::ui::Event;

/// Fixed dropdown of (label, resolver) for log sources.
const SOURCES: &[(&str, &str)] = &[
    ("Application", "app"),
    ("Nginx access", "nginx_access"),
    ("Nginx error", "nginx_error"),
];

pub struct LogsPage {
    pub body: gtk::Widget,
    pub actions: Vec<gtk::Widget>,
    buf: gtk::TextBuffer,
    source_dd: gtk::DropDown,
}

impl LogsPage {
    pub fn build(ctx: &AppCtx) -> Self {
        let reload = gtk::Button::from_icon_name("view-refresh-symbolic");
        reload.set_tooltip_text(Some("Reload"));

        let labels: Vec<&str> = SOURCES.iter().map(|(l, _)| *l).collect();
        let list = StringList::new(&labels);
        let source_dd = gtk::DropDown::builder().model(&list).build();

        let buf = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        let view = gtk::TextView::builder()
            .buffer(&buf)
            .monospace(true)
            .editable(false)
            .wrap_mode(gtk::WrapMode::None)
            .build();

        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        margin_all(&bar, 6);
        bar.append(&gtk::Label::new(Some("Source:")));
        bar.append(&source_dd);
        root.append(&bar);
        root.append(&scrolled(&view));

        // Reload current source.
        let load = |dd: gtk::DropDown, ctx: &AppCtx| {
            let key = SOURCES
                .get(dd.selected() as usize)
                .map(|(_, k)| *k)
                .unwrap_or("app");
            let ctx = ctx.clone();
            let key = key.to_string();
            std::thread::spawn(move || {
                let text = read_log_source(&key);
                let _ = ctx
                    .sender
                    .send(Event::Log { source: key, text });
            });
        };

        {
            let ctx = ctx.clone();
            let dd = source_dd.clone();
            reload.connect_clicked(move |_| load(dd.clone(), &ctx));
        }
        {
            let ctx = ctx.clone();
            let dd = source_dd.clone();
            source_dd.connect_notify_local(Some("selected"), move |_, _| {
                load(dd.clone(), &ctx);
            });
        }

        Self {
            body: root.upcast(),
            actions: vec![reload.upcast()],
            buf,
            source_dd,
        }
    }

    pub fn set_log(&self, source: &str, text: &str) {
        let cur = SOURCES
            .get(self.source_dd.selected() as usize)
            .map(|(_, k)| *k)
            .unwrap_or("app");
        if cur == source {
            self.buf.set_text(text);
        }
    }
}

pub fn read_log_source(key: &str) -> String {
    let path: std::path::PathBuf = match key {
        "nginx_access" => "/var/log/nginx/access.log".into(),
        "nginx_error" => "/var/log/nginx/error.log".into(),
        _ => match lsm_core::App::new() {
            Ok(a) => latest_app_log(&a.paths.logs).unwrap_or_else(|| a.paths.logs.join("app.log")),
            Err(e) => return format!("error: {e}"),
        },
    };
    match std::fs::read_to_string(&path) {
        Ok(t) => tail(&t, 8000),
        Err(_) => format!("(no readable log at {})", path.display()),
    }
}

fn latest_app_log(log_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(log_dir).ok()?;
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if !name.starts_with("app.log") {
                return None;
            }
            let modified = entry.metadata().and_then(|m| m.modified()).ok();
            Some((modified, entry.path()))
        })
        .max_by_key(|(modified, path)| (*modified, path.clone()))
        .map(|(_, path)| path)
}

fn tail(text: &str, n: usize) -> String {
    if text.len() > n {
        text[text.len() - n..].to_string()
    } else {
        text.to_string()
    }
}
