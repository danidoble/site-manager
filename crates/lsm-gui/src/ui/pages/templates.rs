//! Project templates gallery → prefilled New Site wizard.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::ui::widgets::{margin_all, scrolled};
use crate::ui::AppCtx;
use lsm_core::templates::ProjectTemplate;

pub struct TemplatesPage {
    pub body: gtk::Widget,
    pub actions: Vec<gtk::Widget>,
}

impl TemplatesPage {
    pub fn build(ctx: &AppCtx) -> Self {
        let flow = gtk::FlowBox::new();
        flow.set_column_spacing(12);
        flow.set_row_spacing(12);
        flow.set_homogeneous(true);
        flow.set_halign(gtk::Align::Fill);
        flow.set_valign(gtk::Align::Start);
        flow.set_hexpand(true);
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_min_children_per_line(1);
        flow.set_max_children_per_line(12);

        let templates = match lsm_core::App::new() {
            Ok(a) => a.templates(),
            Err(_) => Vec::new(),
        };

        for t in templates {
            flow.insert(&card(ctx, t), -1);
        }

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        margin_all(&flow, 12);
        root.append(&scrolled(&flow));

        Self {
            body: root.upcast(),
            actions: vec![],
        }
    }
}

fn card(ctx: &AppCtx, t: ProjectTemplate) -> gtk::Widget {
    let missing = missing_tools(&t);
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 8);
    frame.set_margin_start(4);
    frame.set_margin_end(4);
    frame.set_valign(gtk::Align::Start);
    frame.set_vexpand(false);
    frame.set_hexpand(true);
    frame.set_size_request(190, -1);

    let icon = gtk::Image::from_icon_name(icon_for(&t));
    icon.set_pixel_size(36);
    icon.set_halign(gtk::Align::Start);

    let title = gtk::Label::new(Some(&display_name(&t.name)));
    title.set_halign(gtk::Align::Start);
    title.add_css_class("heading");

    let sub_text = if missing.is_empty() {
        format!("{} · {}", t.runtime, t.site_type_str)
    } else {
        format!("Requires {}", missing.join(", "))
    };
    let sub = gtk::Label::new(Some(&sub_text));
    sub.set_halign(gtk::Align::Start);
    sub.add_css_class("dim-more");

    let runtime = gtk::Label::new(Some(&format!("{} · {}", t.runtime, t.install)));
    runtime.set_halign(gtk::Align::Start);
    runtime.set_xalign(0.0);
    runtime.set_wrap(true);
    runtime.add_css_class("dim-more");

    let use_btn = gtk::Button::with_label("Use template");
    use_btn.add_css_class("suggested-action");
    use_btn.set_sensitive(missing.is_empty());
    {
        let ctx = ctx.clone();
        let t = t.clone();
        use_btn.connect_clicked(move |_| super::new_site::open(&ctx, Some(t.clone())));
    }

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    top.set_halign(gtk::Align::Start);
    top.append(&icon);
    top.append(&title);

    frame.append(&top);
    frame.append(&sub);
    frame.append(&runtime);
    frame.append(&use_btn);
    frame.upcast()
}

fn missing_tools(t: &ProjectTemplate) -> Vec<String> {
    let required: &[&str] = match t.runtime.as_str() {
        "php" => {
            if t.name == "wordpress" {
                &["php"]
            } else {
                &["php", "composer"]
            }
        }
        "node" => &["node"],
        "python" => {
            if t.name == "django" {
                &["python3", "django-admin"]
            } else {
                &["python3"]
            }
        }
        "go" => &["go"],
        _ => &[],
    };
    let mut missing: Vec<String> = required
        .iter()
        .filter(|tool| find_bin(tool).is_none())
        .map(|tool| (*tool).to_string())
        .collect();
    if t.runtime == "node" && package_managers().is_empty() {
        missing.push("npm/pnpm/yarn/bun".to_string());
    }
    missing
}

fn package_managers() -> Vec<String> {
    ["npm", "pnpm", "yarn", "bun"]
        .iter()
        .filter(|bin| find_bin(bin).is_some())
        .map(|bin| (*bin).to_string())
        .collect()
}

fn find_bin(bin: &str) -> Option<std::path::PathBuf> {
    for dir in candidate_dirs() {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn candidate_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".npm-global/bin"));
        let nvm = home.join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(nvm) {
            for entry in entries.flatten() {
                dirs.push(entry.path().join("bin"));
            }
        }
    }
    dirs.push(std::path::PathBuf::from("/usr/local/go/bin"));
    dirs
}

fn icon_for(t: &ProjectTemplate) -> &'static str {
    match t.runtime.as_str() {
        "php" => "application-x-php-symbolic",
        "node" => "applications-science-symbolic",
        "python" => "applications-engineering-symbolic",
        "go" => "applications-development-symbolic",
        _ => "folder-documents-symbolic",
    }
}

fn display_name(slug: &str) -> String {
    let mut s = String::new();
    for (i, ch) in slug.chars().enumerate() {
        if i == 0 {
            s.extend(ch.to_uppercase());
        } else {
            s.push(ch);
        }
    }
    s
}
