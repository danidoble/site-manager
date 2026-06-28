//! `site-manager` — command-line interface.
//!
//! Thin wrapper over [`lsm_core::App`].

use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};

use lsm_core::domain::{NewSite, SiteType};
use lsm_core::App;

#[derive(Parser, Debug)]
#[command(
    name = "site-manager",
    version,
    about = "Manage local development sites (nginx + SSL + PHP + proxy)"
)]
struct Cli {
    /// Don't execute privileged actions; the privileged helper runs in dry-run mode.
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Site management.
    Site {
        #[command(subcommand)]
        action: SiteAction,
    },
    /// SSL certificate management.
    Ssl {
        #[command(subcommand)]
        action: SslAction,
    },
    /// Local CA management.
    Ca {
        #[command(subcommand)]
        action: CaAction,
    },
    /// Nginx operations.
    Nginx {
        #[command(subcommand)]
        action: NginxAction,
    },
    /// Local service operations.
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// DNS (dnsmasq / hosts) wizards.
    Dns {
        #[command(subcommand)]
        action: DnsAction,
    },
    /// Run diagnostic checks.
    Diagnose,
    /// Show dashboard status.
    Status,
    /// Backup management.
    Backup {
        #[command(subcommand)]
        action: BackupAction,
    },
    /// Project templates.
    Templates,
    /// Start the local REST API server.
    Api {
        /// Override the listen port.
        #[arg(long)]
        port: Option<u16>,
    },
    /// Background worker (invoked by the systemd timer): validates configs, renews
    /// soon-to-expire certs, and probes proxies.
    Background,
}

#[derive(Subcommand, Debug)]
enum SiteAction {
    /// Create a site.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        domain: String,
        #[arg(long, value_delimiter = ',')]
        aliases: Vec<String>,
        #[arg(long)]
        wildcard: bool,
        #[arg(long, value_parser = ["static","php","proxy"])]
        r#type: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        php: Option<String>,
        #[arg(long)]
        proxy: Option<String>,
        #[arg(long)]
        runtime: Option<String>,
        #[arg(long)]
        template: Option<String>,
        /// Issue an SSL certificate immediately.
        #[arg(long)]
        ssl: bool,
        /// Write nginx config, test, and reload after creating.
        #[arg(long)]
        configure: bool,
        /// Add this domain and aliases to /etc/hosts.
        #[arg(long)]
        hosts: bool,
        /// Disable nginx WebSocket upgrade headers for proxy sites.
        #[arg(long)]
        no_websocket: bool,
    },
    /// List sites.
    List {
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long, default_value_t = 50)]
        per_page: usize,
    },
    /// Show one site.
    Show { id_or_name: String },
    /// Delete a site (and remove its nginx config).
    Delete { id_or_name: String },
    /// Write/test/reload nginx config for a site.
    Configure {
        id_or_name: String,
        #[arg(long)]
        ssl: bool,
    },
    /// Open a site in the default browser.
    Open { id_or_name: String },
}

#[derive(Subcommand, Debug)]
enum SslAction {
    /// Create a certificate (for a site or standalone domains).
    Create {
        #[arg(long)]
        site: Option<String>,
        #[arg(long, value_delimiter = ',')]
        domains: Vec<String>,
    },
    /// Renew a certificate by id.
    Renew { id: i64 },
    /// Delete a certificate by id and remove its files.
    Delete { id: i64 },
    /// List certificates.
    List,
}

#[derive(Subcommand, Debug)]
enum CaAction {
    /// Generate the internal CA.
    Init,
    /// Show CA info.
    Show,
    /// Install the CA into a trust store (system if no --browser).
    Install {
        /// Browser NSS store: firefox, chromium, chrome, brave, or all.
        #[arg(long)]
        browser: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum NginxAction {
    /// Detect the active config layout.
    Layout,
    /// Run `nginx -t`.
    Test,
    /// Reload nginx.
    Reload,
}

#[derive(Subcommand, Debug)]
enum ServiceAction {
    /// Run systemctl status for an allowed local service.
    Status { service: String },
    /// Reload an allowed local service.
    Reload { service: String },
    /// Restart an allowed local service.
    Restart { service: String },
    /// Check if the auto-renew timer is enabled.
    Timer,
}

#[derive(Subcommand, Debug)]
enum DnsAction {
    /// Print the dnsmasq drop-in for a TLD.
    Wizard {
        #[arg(long, default_value = "test")]
        tld: String,
    },
    /// Print setup guides (dnsmasq / hosts / wildcards).
    Guides {
        #[arg(long, default_value = "test")]
        tld: String,
    },
    /// Apply the dnsmasq drop-in (privileged).
    Apply {
        #[arg(long, default_value = "test")]
        tld: String,
    },
}

#[derive(Subcommand, Debug)]
enum BackupAction {
    /// Create a backup archive.
    Create,
    /// List backups.
    List,
    /// Restore a backup by name into <storage>/restored.
    Restore { name: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let app = match App::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: failed to initialize: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Apply the global --dry-run flag.
    let mut app = app;
    if cli.dry_run {
        app.config.dry_run = true;
    }

    match run(app, cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut app: App, command: Command) -> Result<()> {
    match command {
        Command::Site { action } => site(&mut app, action),
        Command::Ssl { action } => ssl(&mut app, action),
        Command::Ca { action } => ca(&mut app, action),
        Command::Nginx { action } => nginx(&mut app, action),
        Command::Service { action } => service(&mut app, action),
        Command::Dns { action } => dns(&mut app, action),
        Command::Diagnose => {
            let results = app.diagnose()?;
            print_diagnostics(&results);
            Ok(())
        }
        Command::Status => {
            let status = app.status()?;
            println!("{}", serde_json::to_string_pretty(&status)?);
            Ok(())
        }
        Command::Backup { action } => backup(&mut app, action),
        Command::Templates => {
            let t = app.templates();
            for tpl in t {
                println!("{:<12} {:<8} {}", tpl.name, tpl.runtime, tpl.install);
            }
            Ok(())
        }
        Command::Api { port } => {
            let port = port.unwrap_or(app.config.api_port);
            eprintln!("The REST API ships in the `lsm-api` binary. Listening on :{port} there.");
            eprintln!("Run: cargo run -p lsm-api -- --port {port}");
            Ok(())
        }
        Command::Background => background(&mut app),
    }
}

fn site(app: &mut App, action: SiteAction) -> Result<()> {
    match action {
        SiteAction::Create {
            name,
            domain,
            aliases,
            wildcard,
            r#type,
            path,
            php,
            proxy,
            runtime,
            template,
            ssl,
            configure,
            hosts,
            no_websocket,
        } => {
            let site_type = SiteType::parse(&r#type).context("invalid site type")?;
            let new = NewSite {
                name: name.clone(),
                primary_domain: domain,
                aliases,
                wildcard,
                site_type,
                project_path: path,
                php_version: php,
                proxy_target: proxy,
                websocket: !no_websocket,
                runtime,
                template,
                ..Default::default()
            };
            let site = app.create_site(new).context("create site")?;
            println!("created site `{}` (id={})", site.name, site.id);
            if hosts {
                let res = app.add_hosts_for_site(&site).context("update /etc/hosts")?;
                println!("{}", res.message);
            }
            if ssl {
                let cert = app.issue_site_cert(site.id).context("issue cert")?;
                println!("issued cert id={} for {}", cert.id, cert.domains.join(", "));
            }
            if configure {
                let _ = app
                    .configure_site(site.id, false)
                    .context("configure site")?;
                println!("nginx configured and reloaded");
            }
            Ok(())
        }
        SiteAction::List {
            search,
            page,
            per_page,
        } => {
            let sites = app.list_sites(search.as_deref(), page, per_page)?;
            println!("{}", serde_json::to_string_pretty(&sites)?);
            Ok(())
        }
        SiteAction::Show { id_or_name } => {
            let s = resolve_site(app, &id_or_name)?;
            println!("{}", serde_json::to_string_pretty(&s)?);
            Ok(())
        }
        SiteAction::Delete { id_or_name } => {
            let s = resolve_site(app, &id_or_name)?;
            app.delete_site(s.id).context("delete site")?;
            println!("deleted site `{}`", s.name);
            Ok(())
        }
        SiteAction::Configure { id_or_name, ssl } => {
            let s = resolve_site(app, &id_or_name)?;
            let cert = app.configure_site(s.id, ssl).context("configure site")?;
            if let Some(c) = cert {
                println!("issued cert id={}", c.id);
            }
            println!("nginx configured and reloaded");
            Ok(())
        }
        SiteAction::Open { id_or_name } => {
            let s = resolve_site(app, &id_or_name)?;
            let url = format!("https://{}/", s.primary_domain.trim_start_matches("*."));
            App::open_in_browser(&url)?;
            println!("opening {url}");
            Ok(())
        }
    }
}

fn ssl(app: &mut App, action: SslAction) -> Result<()> {
    match action {
        SslAction::Create { site, domains } => match site {
            Some(id_or_name) => {
                let s = resolve_site(app, &id_or_name)?;
                let cert = app.issue_site_cert(s.id).context("issue cert")?;
                println!("{}", serde_json::to_string_pretty(&cert)?);
                Ok(())
            }
            None => {
                if domains.is_empty() {
                    return Err(anyhow!("provide --site <name> or --domains a,b"));
                }
                let cert = app
                    .issue_domains(None, "standalone", &domains)
                    .context("issue cert")?;
                println!("{}", serde_json::to_string_pretty(&cert)?);
                Ok(())
            }
        },
        SslAction::Renew { id } => {
            let cert = app.renew_cert(id).context("renew cert")?;
            println!("{}", serde_json::to_string_pretty(&cert)?);
            Ok(())
        }
        SslAction::Delete { id } => {
            app.delete_cert(id).context("delete cert")?;
            println!("deleted certificate {id}");
            Ok(())
        }
        SslAction::List => {
            let certs = app.list_certs()?;
            println!("{}", serde_json::to_string_pretty(&certs)?);
            Ok(())
        }
    }
}

fn service(app: &mut App, action: ServiceAction) -> Result<()> {
    let (systemctl_action, service) = match action {
        ServiceAction::Status { service } => ("status", service),
        ServiceAction::Reload { service } => ("reload", service),
        ServiceAction::Restart { service } => ("restart", service),
        ServiceAction::Timer => ("is-enabled", "local-site-manager.timer".to_string()),
    };
    let res = app
        .systemctl(systemctl_action, &service)
        .with_context(|| format!("systemctl {systemctl_action} {service}"))?;
    println!("{}", res.message);
    if !res.stdout.trim().is_empty() {
        println!("{}", res.stdout.trim());
    }
    if !res.stderr.trim().is_empty() {
        eprintln!("{}", res.stderr.trim());
    }
    Ok(())
}

fn ca(app: &mut App, action: CaAction) -> Result<()> {
    match action {
        CaAction::Init => {
            let ca = app.init_ca().context("init CA")?;
            println!("CA initialized: {}", ca.cert_path);
            println!("fingerprint: {}", ca.fingerprint);
            Ok(())
        }
        CaAction::Show => {
            let ca = app.ca_info()?;
            match ca {
                Some(c) => println!("{}", serde_json::to_string_pretty(&c)?),
                None => println!("no CA initialized (run `ca init`)"),
            }
            Ok(())
        }
        CaAction::Install { browser } => {
            let res = app.install_ca(browser.as_deref()).context("install CA")?;
            println!("{}", res.message);
            Ok(())
        }
    }
}

fn nginx(app: &mut App, action: NginxAction) -> Result<()> {
    match action {
        NginxAction::Layout => {
            println!("{}", app.detect_layout().as_str());
            Ok(())
        }
        NginxAction::Test => {
            let (ok, msg) = app.nginx_test().context("nginx test")?;
            println!("nginx -t: {msg}");
            if !ok {
                return Err(anyhow!("nginx config test failed"));
            }
            Ok(())
        }
        NginxAction::Reload => {
            let res = app.nginx_reload().context("nginx reload")?;
            println!("{}", res.message);
            Ok(())
        }
    }
}

fn dns(app: &mut App, action: DnsAction) -> Result<()> {
    match action {
        DnsAction::Wizard { tld } => {
            print!("{}", app.dnsmasq_config(&tld));
            Ok(())
        }
        DnsAction::Guides { tld } => {
            let (d, h, w) = app.dns_guides(&tld);
            println!("{d}\n{h}\n{w}");
            Ok(())
        }
        DnsAction::Apply { tld } => {
            let res = app.apply_dnsmasq(&tld).context("apply dnsmasq")?;
            println!("{}", res.message);
            Ok(())
        }
    }
}

fn backup(app: &mut App, action: BackupAction) -> Result<()> {
    match action {
        BackupAction::Create => {
            let entry = app.backup_create().context("create backup")?;
            println!("{}", serde_json::to_string_pretty(&entry)?);
            Ok(())
        }
        BackupAction::List => {
            let list = app.backup_list()?;
            println!("{}", serde_json::to_string_pretty(&list)?);
            Ok(())
        }
        BackupAction::Restore { name } => {
            let files = app.backup_restore(&name).context("restore backup")?;
            println!("restored {} files", files.len());
            Ok(())
        }
    }
}

fn background(app: &mut App) -> Result<()> {
    // nginx config validation
    match app.nginx_test() {
        Ok((ok, msg)) => println!("nginx -t: {msg} (ok={ok})"),
        Err(e) => println!("nginx -t: error {e}"),
    }
    // Renew certs expiring within 30 days.
    let mut renewed = 0;
    for c in app.list_certs()? {
        if lsm_core::ssl::expiring(&c.not_after, 30) {
            match app.renew_cert(c.id) {
                Ok(nc) => {
                    println!("renewed cert {} -> new id {}", c.id, nc.id);
                    renewed += 1;
                }
                Err(e) => println!("renew cert {}: {e}", c.id),
            }
        }
    }
    println!("renewed {renewed} certificates");
    // Health checks for proxy sites.
    let sites = app.list_sites(None, 1, 500)?;
    for s in sites.iter().filter(|s| s.proxy_target.is_some()) {
        match app.check_proxy(s.id) {
            Ok(h) => println!(
                "health {}: {} ({}ms)",
                s.name,
                h.status_code.unwrap_or(0),
                h.response_ms.unwrap_or(0)
            ),
            Err(e) => println!("health {}: {e}", s.name),
        }
    }
    Ok(())
}

fn resolve_site(app: &mut App, id_or_name: &str) -> Result<lsm_core::domain::Site> {
    if let Ok(id) = id_or_name.parse::<i64>() {
        return app.get_site(id).map_err(|e| anyhow!(e.to_string()));
    }
    app.find_site(id_or_name)
        .map_err(|e| anyhow!(e.to_string()))?
        .ok_or_else(|| anyhow!("site `{id_or_name}` not found"))
}

fn print_diagnostics(results: &[lsm_core::domain::DiagnosticResult]) {
    for r in results {
        let mark = match r.status {
            lsm_core::domain::DiagnosticStatus::Pass => "✓",
            lsm_core::domain::DiagnosticStatus::Warn => "!",
            lsm_core::domain::DiagnosticStatus::Fail => "✗",
        };
        println!("[{mark}] {:<16} {}", r.name, r.message);
        if let Some(d) = &r.detail {
            for line in d.lines() {
                println!("      {line}");
            }
        }
    }
}
