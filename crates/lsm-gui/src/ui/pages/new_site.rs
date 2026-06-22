//! New Site wizard (single-page, progressive disclosure).

use std::cell::RefCell;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;

use gtk4 as gtk;
use gtk::prelude::*;
use gtk::StringList;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::widgets::margin_all;
use crate::ui::{worker_sites, AppCtx, Event};
use lsm_core::domain::{NewSite, SiteType};
use lsm_core::templates::ProjectTemplate;

pub fn open(ctx: &AppCtx, template: Option<ProjectTemplate>) {
    tracing::info!("opening new site dialog");
    let dialog = adw::Dialog::builder()
        .title("New site")
        .content_width(520)
        .build();

    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    header.set_title_widget(Some(&adw::WindowTitle::new("New site", "")));
    let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
    close_btn.add_css_class("flat");
    close_btn.set_tooltip_text(Some("Close"));
    {
        let dialog = dialog.clone();
        close_btn.connect_clicked(move |_| {
            let _ = dialog.close();
        });
    }
    let create_btn = gtk::Button::with_label("Create");
    create_btn.add_css_class("suggested-action");
    header.pack_start(&close_btn);
    header.pack_end(&create_btn);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(560);
    let inner = gtk::Box::new(gtk::Orientation::Vertical, 16);
    margin_all(&inner, 12);
    clamp.set_child(Some(&inner));
    body.append(&clamp);
    toolbar.set_content(Some(&body));

    // Basic group
    let basic = adw::PreferencesGroup::new();
    basic.set_title("Identity");

    let name_e = entry_row("name (e.g. app)", template.as_ref().map(|t| t.name.clone()));
    let domain_e = entry_row("domain (e.g. app.test)", None);
    let aliases_e = entry_row("aliases (comma separated)", None);

    let (type_row, type_dd, type_list) = {
        let list = StringList::new(&["static", "php", "proxy"]);
        let dd = gtk::DropDown::builder().model(&list).build();
        let row = adw::ActionRow::builder().title("Type").build();
        let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        box_.set_valign(gtk::Align::Center);
        box_.append(&dd);
        row.add_suffix(&box_);
        (row, dd, list)
    };

    // Path and backend groups
    let project = adw::PreferencesGroup::new();
    project.set_title("Project");
    let path_e = entry_row("project path (optional)", Some("/var/www/".into()));

    let wildcard_cb = gtk::CheckButton::with_label("Wildcard *.domain");
    let wild_row = adw::ActionRow::builder().title("Wildcards").build();
    wild_row.add_suffix(&wildcard_cb);
    let hosts_cb = gtk::CheckButton::with_label("Add to /etc/hosts");
    let hosts_row = adw::ActionRow::builder()
        .title("/etc/hosts")
        .subtitle("Optional; leave off if you use dnsmasq")
        .build();
    hosts_row.add_suffix(&hosts_cb);

    let mode = adw::PreferencesGroup::new();
    mode.set_title("Backend");
    let php_e = entry_row("php version (e.g. 8.3)", None);
    let proxy_e = entry_row("proxy target (e.g. 127.0.0.1:3000)", None);
    let websocket_cb = gtk::CheckButton::with_label("Enable WebSockets");
    websocket_cb.set_active(true);
    let websocket_row = adw::ActionRow::builder()
        .title("WebSockets")
        .subtitle("Adds nginx Upgrade/Connection headers")
        .build();
    websocket_row.add_suffix(&websocket_cb);

    basic.add(&name_e.0);
    basic.add(&domain_e.0);
    basic.add(&aliases_e.0);
    basic.add(&type_row);
    project.add(&path_e.0);
    project.add(&wild_row);
    project.add(&hosts_row);
    mode.add(&php_e.0);
    mode.add(&proxy_e.0);
    mode.add(&websocket_row);

    inner.append(&basic);
    inner.append(&project);
    inner.append(&mode);

    // Pre-fill from template.
    if let Some(t) = &template {
        let dn = format!("{}.test", t.name);
        domain_e.1.set_text(&dn);
        path_e.1.set_text(&format!("/var/www/{dn}/html"));
        let idx = match t.site_type() {
            SiteType::Static => 0,
            SiteType::Php => 1,
            SiteType::Proxy => 2,
        };
        type_dd.set_selected(idx);
        if matches!(t.site_type(), SiteType::Php) {
            php_e.1.set_text("8.3");
        }
        if matches!(t.site_type(), SiteType::Proxy) {
            proxy_e.1.set_text(default_proxy_target(&t.name, &t.runtime));
        }
    }
    update_backend_visibility(&type_dd, &type_list, &mode, &php_e.0, &proxy_e.0, &websocket_row);
    {
        let last_generated: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let path_row = path_e.1.clone();
        let last_generated = Rc::clone(&last_generated);
        domain_e.1.connect_changed(move |domain_row| {
            let domain = domain_row.text().trim().trim_start_matches("*.").to_string();
            if domain.is_empty() {
                return;
            }
            let next = format!("/var/www/{domain}/html");
            let current = path_row.text().to_string();
            let previous = last_generated.borrow().clone();
            if current.trim().is_empty() || previous.as_deref() == Some(current.as_str()) {
                path_row.set_text(&next);
                *last_generated.borrow_mut() = Some(next);
            }
        });
    }
    {
        let type_list = type_list.clone();
        let mode = mode.clone();
        let php_row = php_e.0.clone();
        let proxy_row = proxy_e.0.clone();
        let websocket_row = websocket_row.clone();
        type_dd.connect_selected_notify(move |dd| {
            update_backend_visibility(dd, &type_list, &mode, &php_row, &proxy_row, &websocket_row);
        });
    }

    {
        let ctx = ctx.clone();
        let name_e = name_e.1.clone();
        let domain_e = domain_e.1.clone();
        let aliases_e = aliases_e.1.clone();
        let php_e = php_e.1.clone();
        let proxy_e = proxy_e.1.clone();
        let path_e = path_e.1.clone();
        let type_dd = type_dd.clone();
        let type_list = type_list.clone();
        let wildcard_cb = wildcard_cb.clone();
        let hosts_cb = hosts_cb.clone();
        let websocket_cb = websocket_cb.clone();
        let template = template.clone();

        create_btn.connect_clicked(move |_| {
            let name = name_e.text().to_string();
            let domain = domain_e.text().to_string();
            let aliases: Vec<String> = aliases_e
                .text()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let tstr = type_list
                .string(type_dd.selected())
                .map(|s| s.to_string())
                .unwrap_or_default();
            let site_type = SiteType::parse(&tstr).unwrap_or(SiteType::Static);
            let php = if site_type == SiteType::Php {
                optional(php_e.text())
            } else {
                None
            };
            let proxy = if site_type == SiteType::Proxy {
                optional(proxy_e.text())
            } else {
                None
            };
            let path = optional(path_e.text());
            let wildcard = wildcard_cb.is_active();
            let add_hosts = hosts_cb.is_active();
            let websocket = site_type == SiteType::Proxy && websocket_cb.is_active();
            let template_name = template.as_ref().map(|t| t.name.clone());

            let new = NewSite {
                name,
                primary_domain: domain,
                aliases,
                wildcard,
                site_type,
                project_path: path,
                php_version: php,
                proxy_target: proxy,
                websocket,
                runtime: None,
                template: template_name.clone(),
            };

            let sender = ctx.sender.clone();
            let template = template.clone();
            std::thread::spawn(move || {
                let result = lsm_core::App::new().and_then(|a| {
                    let site = a.create_site(new)?;
                    if let Some(t) = template.as_ref() {
                        if let Err(e) = scaffold_template(t, &site.project_path) {
                            let _ = a.delete_site(site.id);
                            return Err(lsm_core::Error::Other(e.to_string()));
                        }
                    }
                    if add_hosts {
                        a.add_hosts_for_site(&site)?;
                    }
                    a.configure_site(site.id, false)?;
                    Ok(site)
                });
                let _ = sender.send(match result {
                    Ok(s) => Event::Toast(format!("Created and configured site “{}”", s.name)),
                    Err(e) => Event::Error(e.to_string()),
                });
                let _ = sender.send(worker_sites());
            });
        });
    }

    dialog.set_child(Some(&toolbar));
    dialog.present(Some(&ctx.window));
}

fn scaffold_template(template: &ProjectTemplate, path: &str) -> Result<(), String> {
    let path = Path::new(path);
    if path.exists() && path.read_dir().map_err(|e| e.to_string())?.next().is_some() {
        return Err(format!(
            "project path is not empty: {}. Empty it or choose another path.",
            path.display()
        ));
    }
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;

    match template.name.as_str() {
        "laravel" => run_in_parent(path, "composer", &["create-project", "laravel/laravel"]),
        "symfony" => run_in_parent(path, "composer", &["create-project", "symfony/skeleton"]),
        "statamic" => run_in_parent(path, "composer", &["create-project", "statamic/statamic"]),
        "react" => run_vite(path, "react"),
        "vue" => run_vite(path, "vue"),
        "nextjs" => run_in_parent(path, "npx", &["create-next-app@latest"]),
        "nuxt" => run_in_parent(path, "npx", &["nuxi@latest", "init"]),
        "django" => run_command(path, "django-admin", &["startproject", "app", "."]),
        "flask" => {
            run_command(path, "python3", &["-m", "venv", ".venv"])?;
            std::fs::write(
                path.join("app.py"),
                "from flask import Flask\n\napp = Flask(__name__)\n\n@app.get('/')\ndef index():\n    return 'Hello from Flask'\n",
            )
            .map_err(|e| e.to_string())
        }
        "fastapi" => {
            run_command(path, "python3", &["-m", "venv", ".venv"])?;
            std::fs::write(
                path.join("main.py"),
                "from fastapi import FastAPI\n\napp = FastAPI()\n\n@app.get('/')\ndef index():\n    return {'ok': True}\n",
            )
            .map_err(|e| e.to_string())
        }
        "go-fiber" => {
            run_command(path, "go", &["mod", "init", "local/site"])?;
            run_command(path, "go", &["get", "github.com/gofiber/fiber/v2"])?;
            std::fs::write(
                path.join("main.go"),
                "package main\n\nimport \"github.com/gofiber/fiber/v2\"\n\nfunc main() {\n\tapp := fiber.New()\n\tapp.Get(\"/\", func(c *fiber.Ctx) error { return c.SendString(\"Hello from Fiber\") })\n\tapp.Listen(\":3000\")\n}\n",
            )
            .map_err(|e| e.to_string())
        }
        "go-gin" => {
            run_command(path, "go", &["mod", "init", "local/site"])?;
            run_command(path, "go", &["get", "github.com/gin-gonic/gin"])?;
            std::fs::write(
                path.join("main.go"),
                "package main\n\nimport \"github.com/gin-gonic/gin\"\n\nfunc main() {\n\tr := gin.Default()\n\tr.GET(\"/\", func(c *gin.Context) { c.String(200, \"Hello from Gin\") })\n\tr.Run(\":3000\")\n}\n",
            )
            .map_err(|e| e.to_string())
        }
        "wordpress" => Err("wordpress scaffold is not automatic yet; download wordpress into the project path".into()),
        other => Err(format!("no scaffold command configured for template `{other}`")),
    }
}

fn run_in_parent(path: &Path, bin: &str, args: &[&str]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "project path has no parent".to_string())?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "project path has no final directory name".to_string())?;
    let _ = std::fs::remove_dir(path);
    let mut full_args: Vec<&str> = args.to_vec();
    full_args.push(name);
    run_command(parent, bin, &full_args)
}

fn run_vite(path: &Path, template: &str) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "project path has no parent".to_string())?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "project path has no final directory name".to_string())?;
    let _ = std::fs::remove_dir(path);
    run_command(parent, "npm", &["create", "vite@latest", name, "--", "--template", template])
}

fn run_command(cwd: &Path, bin: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("spawn {bin}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        Err(format!(
            "{} {} failed: {}{}",
            bin,
            args.join(" "),
            stderr.trim(),
            if stdout.trim().is_empty() { "" } else { stdout.trim() }
        ))
    }
}

fn update_backend_visibility(
    type_dd: &gtk::DropDown,
    type_list: &StringList,
    mode: &adw::PreferencesGroup,
    php_row: &adw::EntryRow,
    proxy_row: &adw::EntryRow,
    websocket_row: &adw::ActionRow,
) {
    let tstr = type_list
        .string(type_dd.selected())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let site_type = SiteType::parse(&tstr).unwrap_or(SiteType::Static);
    php_row.set_visible(site_type == SiteType::Php);
    proxy_row.set_visible(site_type == SiteType::Proxy);
    websocket_row.set_visible(site_type == SiteType::Proxy);
    mode.set_visible(site_type != SiteType::Static);
}

fn optional(s: glib::GString) -> Option<String> {
    let s = s.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn default_proxy_target(name: &str, runtime: &str) -> &'static str {
    match (name, runtime) {
        ("django" | "flask" | "fastapi", _) | (_, "python") => "127.0.0.1:8000",
        ("go-fiber" | "go-gin", _) | (_, "go") => "127.0.0.1:3000",
        _ => "127.0.0.1:3000",
    }
}

/// Build a full-width entry row; return (row, editable handle).
fn entry_row(placeholder: &str, initial: Option<String>) -> (adw::EntryRow, adw::EntryRow) {
    let e = adw::EntryRow::builder().title(placeholder).build();
    e.set_hexpand(true);
    if let Some(init) = initial {
        e.set_text(&init);
    }
    (e.clone(), e)
}
