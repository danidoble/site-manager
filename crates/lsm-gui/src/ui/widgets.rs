//! Reusable GNOME-style widget builders + app CSS.

use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

/// Visual kind for status pills / icons.
#[derive(Clone, Copy)]
pub enum Kind {
    Success,
    Warning,
    Error,
    Inactive,
}

impl Kind {
    fn class(self) -> &'static str {
        match self {
            Kind::Success => "success",
            Kind::Warning => "warning",
            Kind::Error => "error",
            Kind::Inactive => "inactive",
        }
    }
}

/// Pill/card styling. Accent colors are left to libadwaita/system settings.
pub const CSS: &str = "
/* Status pills */
.pill {
  border-radius: 9999px;
  padding: 2px 10px;
  font-weight: 700;
}
.pill.success  { background-color: alpha(@success_bg_color,0.35); color: @success_color; }
.pill.warning  { background-color: alpha(@warning_bg_color,0.35); color: @warning_color; }
.pill.error    { background-color: alpha(@error_bg_color,0.35);   color: @error_color; }
.pill.inactive { background-color: alpha(@window_fg_color,0.10);  color: alpha(@window_fg_color,0.75); }

/* Dashboard stat cards */
.stat-card {
  background-color: @card_bg_color;
  border-radius: 8px;
  padding: 18px;
}
.stat-value { font-size: 1.9rem; font-weight: 800; }
.stat-title { font-weight: 700; color: alpha(@window_fg_color,0.7); }
.stat-sub   { color: alpha(@window_fg_color,0.55); }

/* Monospace info block */
.code-block {
  background-color: alpha(@window_fg_color,0.06);
  border-radius: 10px;
  padding: 10px 12px;
}

/* Sidebar nav row spacing */
.nav-list { padding: 6px; }
.dim-more { color: alpha(@window_fg_color,0.55); }
";

/// Load the app CSS onto the window's display.
pub fn load_css(display: &gtk::gdk::Display) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(CSS);
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
    );
}

/// Small rounded status badge: `pill success "Valid"`.
pub fn pill(kind: Kind, text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("pill");
    l.add_css_class(kind.class());
    l
}

/// Dashboard stat card: big value over title + subtitle.
pub fn stat_card(title: &str, value: &str, subtitle: &str) -> gtk::Widget {
    let boxv = gtk::Box::new(gtk::Orientation::Vertical, 4);
    boxv.add_css_class("stat-card");

    let t = gtk::Label::new(Some(title));
    t.add_css_class("stat-title");
    t.set_halign(gtk::Align::Start);
    t.set_xalign(0.0);

    let v = gtk::Label::new(Some(value));
    v.add_css_class("stat-value");
    v.set_halign(gtk::Align::Start);
    v.set_xalign(0.0);

    let s = gtk::Label::new(Some(subtitle));
    s.add_css_class("stat-sub");
    s.set_halign(gtk::Align::Start);
    s.set_xalign(0.0);

    boxv.append(&t);
    boxv.append(&v);
    boxv.append(&s);
    boxv.upcast()
}

/// Monospace, copyable block for paths / domains / config.
#[allow(dead_code)]
pub fn code_block(text: &str) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 4);

    let label = gtk::Label::new(Some(text));
    label.add_css_class("code-block");
    label.add_css_class("monospace");
    label.set_selectable(true);
    label.set_xalign(0.0);
    label.set_halign(gtk::Align::Start);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::Char);
    label.set_hexpand(true);

    let copy = gtk::Button::with_label("Copy");
    copy.add_css_class("flat");
    copy.set_halign(gtk::Align::End);
    {
        let label = label.clone();
        let text = text.to_string();
        copy.connect_clicked(move |_| {
            label.clipboard().set_text(&text);
        });
    }

    row.append(&label);
    row.append(&copy);
    row.upcast()
}

/// Friendly empty-state panel.
pub fn empty_state(title: &str, description: &str, icon: &str) -> adw::StatusPage {
    let s = adw::StatusPage::new();
    s.set_icon_name(Some(icon));
    s.set_title(title);
    s.set_description(Some(description));
    s.set_vexpand(true);
    s
}

/// Sidebar navigation row.
pub fn nav_row(title: &str, icon: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).activatable(true).build();
    let img = gtk::Image::from_icon_name(icon);
    row.add_prefix(&img);
    row
}

pub fn margin_all(w: &impl IsA<gtk::Widget>, n: i32) {
    w.set_margin_start(n);
    w.set_margin_end(n);
    w.set_margin_top(n);
    w.set_margin_bottom(n);
}

/// Scrollable wrapper that expands vertically.
pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let sw = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .build();
    sw.set_child(Some(child));
    sw
}

/// Strip all children from a ListBox before repopulating.
pub fn clear_listbox(lb: &gtk::ListBox) {
    let mut child = lb.first_child();
    while let Some(c) = child {
        let next = c.next_sibling();
        lb.remove(&c);
        child = next;
    }
}

/// Strip all children from a generic container box.
pub fn clear_box(b: &gtk::Box) {
    let mut child = b.first_child();
    while let Some(c) = child {
        let next = c.next_sibling();
        b.remove(&c);
        child = next;
    }
}
