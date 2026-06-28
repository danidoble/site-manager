//! Diagnostics: pass/warn/fail checklist + report copy.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4 as gtk;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::widgets::{self, clear_listbox, margin_all, scrolled};
use crate::ui::{worker_diagnostics, AppCtx};
use lsm_core::domain::{DiagnosticResult, DiagnosticStatus};

pub struct DiagPage {
    pub body: gtk::Widget,
    pub actions: Vec<gtk::Widget>,
    pub list: gtk::ListBox,
    report: Rc<RefCell<String>>,
}

impl DiagPage {
    pub fn build(ctx: &AppCtx) -> Self {
        let run = gtk::Button::with_label("Run diagnostics");
        run.add_css_class("suggested-action");
        let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
        copy.set_tooltip_text(Some("Copy report"));

        {
            let ctx = ctx.clone();
            run.connect_clicked(move |_| ctx.spawn(worker_diagnostics));
        }

        let report: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
        {
            let report = Rc::clone(&report);
            copy.connect_clicked(move |btn| {
                let text = report.borrow().clone();
                if !text.is_empty() {
                    btn.clipboard().set_text(&text);
                }
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
            actions: vec![copy.upcast(), run.upcast()],
            list,
            report,
        }
    }

    pub fn set_diagnostics(&self, results: Vec<DiagnosticResult>) {
        clear_listbox(&self.list);
        if results.is_empty() {
            let empty = widgets::empty_state(
                "No diagnostics yet",
                "Click “Run diagnostics” to check nginx, DNS, SSL, PHP and permissions.",
                "system-run-symbolic",
            );
            self.list
                .set_placeholder(Some(empty.upcast_ref::<gtk::Widget>()));
            self.report.borrow_mut().clear();
            return;
        }

        use std::fmt::Write;
        let mut report = String::new();
        for d in &results {
            let mark = match d.status {
                DiagnosticStatus::Pass => "PASS",
                DiagnosticStatus::Warn => "WARN",
                DiagnosticStatus::Fail => "FAIL",
            };
            let _ = writeln!(report, "[{mark}] {} — {}", d.name, d.message);
            self.list.append(&row(d));
        }
        *self.report.borrow_mut() = report;
    }
}

fn row(d: &DiagnosticResult) -> gtk::Widget {
    let (icon, label) = match d.status {
        DiagnosticStatus::Pass => ("emblem-ok-symbolic", "Pass"),
        DiagnosticStatus::Warn => ("dialog-warning-symbolic", "Warn"),
        DiagnosticStatus::Fail => ("dialog-error-symbolic", "Fail"),
    };
    let r = adw::ActionRow::builder()
        .title(&d.name)
        .subtitle(&d.message)
        .build();
    r.add_prefix(&gtk::Image::from_icon_name(icon));
    let text = gtk::Label::new(Some(label));
    text.add_css_class("dim-label");
    r.add_suffix(&text);
    r.upcast()
}
