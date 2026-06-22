//! Project templates gallery → prefilled New Site wizard.

use gtk4 as gtk;
use gtk::prelude::*;

use crate::ui::widgets::{margin_all, scrolled, Kind};
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
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 8);
    frame.add_css_class("stat-card");
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

    let sub = gtk::Label::new(Some(&format!("{} · {}", t.runtime, t.site_type_str)));
    sub.set_halign(gtk::Align::Start);
    sub.add_css_class("dim-more");

    let badge = crate::ui::widgets::pill(Kind::Inactive, &t.runtime);

    let use_btn = gtk::Button::with_label("Use template");
    use_btn.add_css_class("suggested-action");
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
    frame.append(&badge);
    frame.append(&sub);
    frame.append(&use_btn);
    frame.upcast()
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
