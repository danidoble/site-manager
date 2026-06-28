//! `local-site-manager-api` — local REST API on :5847.
//!
//! All handlers delegate to [`lsm_core::App`] behind a mutex, executed on the
//! blocking thread pool so SQLite calls never stall the async runtime.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use tower_http::trace::TraceLayer;

use lsm_core::domain::{DiagnosticResult, NewSite, Site, SslCertificate, Status};
use lsm_core::{App, Error};

type Shared = Arc<Mutex<App>>;

#[derive(Clone)]
struct AppState {
    app: Shared,
}

/// Local error wrapper so we may implement `IntoResponse` (orphan rule).
struct ApiError(Error);

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = match &self.0 {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Validation(_) | Error::Nginx(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (code, Json(json!({ "error": self.0.to_string() }))).into_response()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "--port")
        .and_then(|w| w[1].parse::<u16>().ok())
        .unwrap_or_else(|| {
            App::new()
                .map(|a| a.config.api_port)
                .unwrap_or(lsm_core::config::DEFAULT_API_PORT)
        });

    let app = App::new()?;
    let bind = format!("127.0.0.1:{port}");
    let state = AppState {
        app: Arc::new(Mutex::new(app)),
    };

    let router = routes(state).layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("lsm-api listening on http://{bind}");
    axum::serve(listener, router).await?;
    Ok(())
}

fn routes(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/api/diagnostics", get(diagnostics))
        .route("/api/templates", get(templates))
        // sites
        .route("/api/sites", get(list_sites).post(create_site))
        .route("/api/sites/:id", get(get_site).delete(delete_site))
        .route("/api/sites/:id/configure", post(configure_site))
        .route("/api/sites/:id/cert", post(site_cert))
        .route("/api/sites/:id/health", get(site_health))
        // certs / ssl
        .route("/api/certs", get(list_certs))
        .route("/api/certs/:id/renew", post(renew_cert))
        .route("/api/ssl/create", post(ssl_create))
        .route("/api/ssl/renew", post(ssl_renew))
        // nginx
        .route("/api/nginx/test", get(nginx_test))
        .route("/api/nginx/reload", post(nginx_reload))
        // backups
        .route("/api/backups", get(list_backups).post(create_backup))
        .route("/api/backups/:name/restore", post(restore_backup))
        .with_state(state)
}

// ---- handlers ----

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true, "service": "local-site-manager-api", "version": lsm_core::VERSION }))
}

#[derive(Deserialize)]
struct PageQuery {
    #[serde(default)]
    search: Option<String>,
    #[serde(default = "default_page")]
    page: usize,
    #[serde(default = "default_per_page")]
    per_page: usize,
}
fn default_page() -> usize {
    1
}
fn default_per_page() -> usize {
    50
}

async fn status(State(s): State<AppState>) -> Result<Json<Status>, ApiError> {
    run(s, |a| a.status()).await.map(Json)
}

async fn diagnostics(State(s): State<AppState>) -> Result<Json<Vec<DiagnosticResult>>, ApiError> {
    run(s, |a| a.diagnose()).await.map(Json)
}

async fn templates(State(s): State<AppState>) -> Json<Vec<lsm_core::templates::ProjectTemplate>> {
    Json(s.app.lock().expect("mutex").templates())
}

async fn list_sites(
    State(s): State<AppState>,
    Query(q): Query<PageQuery>,
) -> Result<Json<Vec<Site>>, ApiError> {
    run(s, move |a| {
        a.list_sites(q.search.as_deref(), q.page, q.per_page)
    })
    .await
    .map(Json)
}

async fn create_site(
    State(s): State<AppState>,
    Json(body): Json<NewSite>,
) -> Result<(StatusCode, Json<Site>), ApiError> {
    let site = run(s, move |a| a.create_site(body)).await?;
    Ok((StatusCode::CREATED, Json(site)))
}

async fn get_site(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<Site>, ApiError> {
    run(s, move |a| a.get_site(id)).await.map(Json)
}

async fn delete_site(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    run(s, move |a| a.delete_site(id)).await?;
    Ok(Json(json!({ "deleted": id })))
}

#[derive(Deserialize)]
struct ConfigureBody {
    #[serde(default)]
    ssl: bool,
}
async fn configure_site(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ConfigureBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cert = run(s, move |a| a.configure_site(id, body.ssl)).await?;
    Ok(Json(
        json!({ "configured": id, "cert": cert.map(|c| c.id) }),
    ))
}

async fn site_cert(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<SslCertificate>, ApiError> {
    run(s, move |a| a.issue_site_cert(id)).await.map(Json)
}

async fn site_health(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<lsm_core::domain::HealthCheck>, ApiError> {
    run(s, move |a| a.check_proxy(id)).await.map(Json)
}

async fn list_certs(State(s): State<AppState>) -> Result<Json<Vec<SslCertificate>>, ApiError> {
    run(s, |a| a.list_certs()).await.map(Json)
}

async fn renew_cert(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<SslCertificate>, ApiError> {
    run(s, move |a| a.renew_cert(id)).await.map(Json)
}

#[derive(Deserialize)]
struct SslCreate {
    site: Option<String>,
    #[serde(default)]
    domains: Vec<String>,
}
async fn ssl_create(
    State(s): State<AppState>,
    Json(body): Json<SslCreate>,
) -> Result<Json<SslCertificate>, ApiError> {
    run(s, move |a| match &body.site {
        Some(id_or_name) => {
            let site = resolve(a, id_or_name)?;
            a.issue_site_cert(site.id)
        }
        None => a.issue_domains(None, "standalone", &body.domains),
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
struct SslRenew {
    id: i64,
}
async fn ssl_renew(
    State(s): State<AppState>,
    Json(body): Json<SslRenew>,
) -> Result<Json<SslCertificate>, ApiError> {
    run(s, move |a| a.renew_cert(body.id)).await.map(Json)
}

async fn nginx_test(State(s): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let (ok, msg) = run(s, |a| a.nginx_test()).await?;
    Ok(Json(json!({ "ok": ok, "message": msg })))
}

async fn nginx_reload(
    State(s): State<AppState>,
) -> Result<Json<lsm_core::PrivilegedResult>, ApiError> {
    run(s, |a| a.nginx_reload()).await.map(Json)
}

async fn list_backups(
    State(s): State<AppState>,
) -> Result<Json<Vec<lsm_core::domain::BackupEntry>>, ApiError> {
    run(s, |a| a.backup_list()).await.map(Json)
}

async fn create_backup(
    State(s): State<AppState>,
) -> Result<Json<lsm_core::domain::BackupEntry>, ApiError> {
    run(s, |a| a.backup_create()).await.map(Json)
}

async fn restore_backup(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let files = run(s, move |a| a.backup_restore(&name)).await?;
    Ok(Json(json!({ "restored": files.len() })))
}

// ---- helpers ----

async fn run<F, R>(s: AppState, f: F) -> Result<R, ApiError>
where
    F: FnOnce(&App) -> Result<R, Error> + Send + 'static,
    R: Send + 'static,
{
    let app = s.app;
    tokio::task::spawn_blocking(move || {
        let app = app.lock().expect("app mutex poisoned");
        f(&app)
    })
    .await
    .map_err(|e| ApiError(Error::Other(format!("join: {e}"))))?
    .map_err(ApiError)
}

fn resolve(a: &App, id_or_name: &str) -> Result<Site, Error> {
    if let Ok(id) = id_or_name.parse::<i64>() {
        return a.get_site(id);
    }
    a.find_site(id_or_name)?
        .ok_or_else(|| Error::NotFound(format!("site {id_or_name}")))
}
