//! New Site wizard (single-page, progressive disclosure).

use std::cell::RefCell;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

use gtk::StringList;
use gtk4 as gtk;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::widgets::{margin_all, scrolled};
use crate::ui::{worker_sites, AppCtx, Event};
use lsm_core::domain::{NewSite, SiteType};
use lsm_core::templates::ProjectTemplate;

pub fn open(ctx: &AppCtx, template: Option<ProjectTemplate>) {
    tracing::info!("opening new site dialog");
    let dialog = adw::Dialog::builder()
        .title("New site")
        .content_width(520)
        .content_height(640)
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

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(560);
    let inner = gtk::Box::new(gtk::Orientation::Vertical, 16);
    margin_all(&inner, 12);
    inner.set_size_request(-1, 520);
    clamp.set_child(Some(&inner));
    let scroll = scrolled(&clamp);
    scroll.set_min_content_height(560);
    toolbar.set_content(Some(&scroll));

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
    let path_e = entry_row("project directory (optional)", Some("/var/www/".into()));
    let entry_e = entry_row("web root folder (optional)", Some("html".into()));

    let wildcard_cb = gtk::CheckButton::with_label("Wildcard *.domain");
    let wild_row = adw::ActionRow::builder().title("Wildcards").build();
    wild_row.add_suffix(&wildcard_cb);
    let hosts_cb = gtk::CheckButton::with_label("Add to /etc/hosts");
    let hosts_row = adw::ActionRow::builder()
        .title("/etc/hosts")
        .subtitle("Optional; leave off if you use dnsmasq")
        .build();
    hosts_row.add_suffix(&hosts_cb);
    let ssl_cb = gtk::CheckButton::with_label("Use SSL");
    ssl_cb.set_active(true);
    let ssl_row = adw::ActionRow::builder()
        .title("SSL")
        .subtitle("Issue a local certificate while creating the site")
        .build();
    ssl_row.add_suffix(&ssl_cb);

    let mode = adw::PreferencesGroup::new();
    mode.set_title("Backend");
    let php_e = entry_row("php version (e.g. 8.3)", None);
    let proxy_e = entry_row("proxy target (e.g. 127.0.0.1:3000)", None);
    let package_managers = package_managers();
    let pm_items: Vec<&str> = package_managers.iter().map(String::as_str).collect();
    let pm_list = StringList::new(&pm_items);
    let pm_dd = gtk::DropDown::builder().model(&pm_list).build();
    let pm_row = adw::ActionRow::builder()
        .title("Package manager")
        .subtitle("Used by Node templates")
        .build();
    let pm_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    pm_box.set_valign(gtk::Align::Center);
    pm_box.append(&pm_dd);
    pm_row.add_suffix(&pm_box);
    pm_row.set_visible(template.as_ref().is_some_and(|t| t.runtime == "node"));
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
    project.add(&entry_e.0);
    project.add(&wild_row);
    project.add(&hosts_row);
    project.add(&ssl_row);
    mode.add(&php_e.0);
    mode.add(&pm_row);
    mode.add(&proxy_e.0);
    mode.add(&websocket_row);

    inner.append(&basic);
    inner.append(&project);
    inner.append(&mode);

    // Pre-fill from template.
    if let Some(t) = &template {
        let dn = format!("{}.test", t.name);
        domain_e.1.set_text(&dn);
        path_e.1.set_text(&format!("/var/www/{dn}"));
        entry_e.1.set_text(default_entrypoint(t));
        let idx = match t.site_type() {
            SiteType::Static => 0,
            SiteType::Php => 1,
            SiteType::Proxy => 2,
        };
        type_dd.set_selected(idx);
        if matches!(t.site_type(), SiteType::Php) {
            php_e
                .1
                .set_text(&default_php_version().unwrap_or_else(|| "8.3".to_string()));
        }
        if matches!(t.site_type(), SiteType::Proxy) {
            proxy_e
                .1
                .set_text(default_proxy_target(&t.name, &t.runtime));
        }
    }
    update_backend_visibility(
        &type_dd,
        &type_list,
        &mode,
        &php_e.0,
        &proxy_e.0,
        &websocket_row,
    );
    {
        let last_generated: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let domain_row = domain_e.1.clone();
        let last_generated = Rc::clone(&last_generated);
        name_e.1.connect_changed(move |name_row| {
            let slug = slugify(&name_row.text());
            if slug.is_empty() {
                return;
            }
            let next = format!("{slug}.test");
            let current = domain_row.text().to_string();
            let previous = last_generated.borrow().clone();
            if current.trim().is_empty() || previous.as_deref() == Some(current.as_str()) {
                domain_row.set_text(&next);
                *last_generated.borrow_mut() = Some(next);
            }
        });
    }
    {
        let last_generated: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let path_row = path_e.1.clone();
        let last_generated = Rc::clone(&last_generated);
        domain_e.1.connect_changed(move |domain_row| {
            let domain = domain_row
                .text()
                .trim()
                .trim_start_matches("*.")
                .to_string();
            if domain.is_empty() {
                return;
            }
            let next = format!("/var/www/{domain}");
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
        let pm_dd = pm_dd.clone();
        let pm_list = pm_list.clone();
        let path_e = path_e.1.clone();
        let entry_e = entry_e.1.clone();
        let type_dd = type_dd.clone();
        let type_list = type_list.clone();
        let wildcard_cb = wildcard_cb.clone();
        let hosts_cb = hosts_cb.clone();
        let ssl_cb = ssl_cb.clone();
        let websocket_cb = websocket_cb.clone();
        let template = template.clone();
        let dialog = dialog.clone();

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
            let php_for_template = php.clone();
            let package_manager = pm_list
                .string(pm_dd.selected())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "npm".to_string());
            let proxy = if site_type == SiteType::Proxy {
                optional(proxy_e.text())
            } else {
                None
            };
            let project_dir = optional(path_e.text());
            let path = project_dir
                .as_deref()
                .map(|base| web_root_path(base, &entry_e.text()));
            let wildcard = wildcard_cb.is_active();
            let add_hosts = hosts_cb.is_active();
            let use_ssl = ssl_cb.is_active();
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
            let scaffold_dir = project_dir.clone();
            let _ = dialog.close();
            std::thread::spawn(move || {
                let result = lsm_core::App::new().and_then(|a| {
                    let site = a.create_site(new)?;
                    if let Some(t) = template.as_ref() {
                        let scaffold_dir = scaffold_dir.as_deref().unwrap_or(&site.project_path);
                        if let Err(e) = scaffold_template(
                            t,
                            scaffold_dir,
                            php_for_template.as_deref(),
                            &package_manager,
                        ) {
                            let _ = a.delete_site(site.id);
                            return Err(lsm_core::Error::Other(e.to_string()));
                        }
                    }
                    if add_hosts {
                        a.add_hosts_for_site(&site)?;
                    }
                    a.configure_site(site.id, use_ssl)?;
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

fn scaffold_template(
    template: &ProjectTemplate,
    path: &str,
    php_version: Option<&str>,
    package_manager: &str,
) -> Result<(), String> {
    let path = Path::new(path);
    if path.exists() && path.read_dir().map_err(|e| e.to_string())?.next().is_some() {
        return Err(format!(
            "project path is not empty: {}. Empty it or choose another path.",
            path.display()
        ));
    }
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;

    match template.name.as_str() {
        "laravel" => run_composer_create(path, "laravel/laravel", php_version),
        "symfony" => run_composer_create(path, "symfony/skeleton", php_version),
        "statamic" => run_composer_create(path, "statamic/statamic", php_version),
        "react" => run_vite(path, "react", package_manager),
        "vue" => run_vite(path, "vue", package_manager),
        "nextjs" => run_nextjs(path, package_manager),
        "nuxt" => run_nuxt(path, package_manager),
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
        "wordpress" => Err(
            "wordpress scaffold is not automatic yet; download wordpress into the project path"
                .into(),
        ),
        other => Err(format!(
            "no scaffold command configured for template `{other}`"
        )),
    }
}

fn run_composer_create(
    path: &Path,
    package: &str,
    php_version: Option<&str>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "project path has no parent".to_string())?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "project path has no final directory name".to_string())?;
    let _ = std::fs::remove_dir(path);

    if let Some(version) = php_version.map(str::trim).filter(|v| !v.is_empty()) {
        let php_bin = format!("php{version}");
        let composer = which_bin("composer")
            .ok_or_else(|| "composer not found on PATH".to_string())?
            .to_string_lossy()
            .to_string();
        run_command_owned(
            parent,
            &php_bin,
            &[
                composer,
                "create-project".to_string(),
                package.to_string(),
                name.to_string(),
            ],
        )
    } else {
        run_command(parent, "composer", &["create-project", package, name])
    }
}

fn run_vite(path: &Path, template: &str, package_manager: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "project path has no parent".to_string())?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "project path has no final directory name".to_string())?;
    let _ = std::fs::remove_dir(path);
    match package_manager {
        "pnpm" => run_command(
            parent,
            "pnpm",
            &["create", "vite", name, "--template", template],
        ),
        "yarn" => run_command(
            parent,
            "yarn",
            &["create", "vite", name, "--template", template],
        ),
        "bun" => run_command(
            parent,
            "bun",
            &["create", "vite", name, "--template", template],
        ),
        _ => run_command(
            parent,
            "npm",
            &["create", "vite@latest", name, "--", "--template", template],
        ),
    }
}

fn run_nextjs(path: &Path, package_manager: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "project path has no parent".to_string())?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "project path has no final directory name".to_string())?;
    let _ = std::fs::remove_dir(path);
    match package_manager {
        "pnpm" => run_command(parent, "pnpm", &["create", "next-app", name]),
        "yarn" => run_command(parent, "yarn", &["create", "next-app", name]),
        "bun" => run_command(parent, "bun", &["create", "next-app", name]),
        _ => run_command(parent, "npx", &["create-next-app@latest", name]),
    }
}

fn run_nuxt(path: &Path, package_manager: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "project path has no parent".to_string())?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "project path has no final directory name".to_string())?;
    let _ = std::fs::remove_dir(path);
    match package_manager {
        "pnpm" => run_command(parent, "pnpm", &["dlx", "nuxi@latest", "init", name]),
        "yarn" => run_command(parent, "yarn", &["dlx", "nuxi@latest", "init", name]),
        "bun" => run_command(parent, "bunx", &["nuxi@latest", "init", name]),
        _ => run_command(parent, "npx", &["nuxi@latest", "init", name]),
    }
}

fn run_command(cwd: &Path, bin: &str, args: &[&str]) -> Result<(), String> {
    let resolved = find_bin(bin).unwrap_or_else(|| PathBuf::from(bin));
    let mut command = Command::new(&resolved);
    prepare_command(&mut command, cwd);
    let out = command
        .args(args)
        .output()
        .map_err(|e| format!("spawn {}: {e}", resolved.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        Err(format!(
            "{} {} failed: {}{}",
            resolved.display(),
            args.join(" "),
            stderr.trim(),
            if stdout.trim().is_empty() {
                ""
            } else {
                stdout.trim()
            }
        ))
    }
}

fn run_command_owned(cwd: &Path, bin: &str, args: &[String]) -> Result<(), String> {
    let resolved = find_bin(bin).unwrap_or_else(|| PathBuf::from(bin));
    let mut command = Command::new(&resolved);
    prepare_command(&mut command, cwd);
    let out = command
        .args(args)
        .output()
        .map_err(|e| format!("spawn {}: {e}", resolved.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        Err(format!(
            "{} {} failed: {}{}",
            resolved.display(),
            args.join(" "),
            stderr.trim(),
            if stdout.trim().is_empty() {
                ""
            } else {
                stdout.trim()
            }
        ))
    }
}

fn prepare_command(command: &mut Command, cwd: &Path) {
    command.current_dir(cwd);
    if let Ok(path) = std::env::join_paths(candidate_dirs()) {
        command.env("PATH", path);
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
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn default_proxy_target(name: &str, runtime: &str) -> &'static str {
    match (name, runtime) {
        ("django" | "flask" | "fastapi", _) | (_, "python") => "127.0.0.1:8000",
        ("go-fiber" | "go-gin", _) | (_, "go") => "127.0.0.1:3000",
        _ => "127.0.0.1:3000",
    }
}

fn default_entrypoint(template: &ProjectTemplate) -> &'static str {
    match template.name.as_str() {
        "laravel" | "symfony" | "statamic" => "public",
        _ => "",
    }
}

fn web_root_path(base: &str, entrypoint: &glib::GString) -> String {
    let base = base.trim().trim_end_matches('/');
    let entrypoint = entrypoint.trim().trim_matches('/');
    if entrypoint.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{entrypoint}")
    }
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn default_php_version() -> Option<String> {
    let app = lsm_core::App::new().ok()?;
    app.config.php_versions.first().cloned().or_else(|| {
        lsm_core::diagnostics::detect_php_fpm_versions()
            .into_iter()
            .next()
    })
}

fn package_managers() -> Vec<String> {
    let found: Vec<String> = ["npm", "pnpm", "yarn", "bun"]
        .iter()
        .filter(|bin| find_bin(bin).is_some())
        .map(|bin| (*bin).to_string())
        .collect();
    if found.is_empty() {
        vec!["npm".to_string()]
    } else {
        found
    }
}

fn which_bin(bin: &str) -> Option<PathBuf> {
    find_bin(bin)
}

fn find_bin(bin: &str) -> Option<PathBuf> {
    for dir in candidate_dirs() {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".npm-global/bin"));
        let nvm = home.join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(nvm) {
            for entry in entries.flatten() {
                dirs.push(entry.path().join("bin"));
            }
        }
    }
    dirs.push(PathBuf::from("/usr/local/go/bin"));
    dirs
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
