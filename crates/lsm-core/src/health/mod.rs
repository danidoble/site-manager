//! Proxy health probing (specs §Health Monitoring).

use std::time::{Duration, Instant};

use crate::domain::HealthCheck;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Probe a `host:port` proxy target over HTTP.
pub fn probe(site_id: i64, target: &str) -> HealthCheck {
    let url = format!("http://{target}/");
    let client = match reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(e) => return fail(site_id, &format!("build client: {e}")),
    };

    let start = Instant::now();
    match client.get(&url).send() {
        Ok(resp) => {
            let ms = start.elapsed().as_millis() as u64;
            let code = resp.status().as_u16();
            let healthy = (200..400).contains(&code) || (400..500).contains(&code);
            HealthCheck {
                site_id,
                status_code: Some(code),
                healthy,
                response_ms: Some(ms),
                checked_at: now_rfc3339(),
                error: None,
            }
        }
        Err(e) => fail(site_id, &format!("request: {e}")),
    }
}

fn fail(site_id: i64, msg: &str) -> HealthCheck {
    HealthCheck {
        site_id,
        status_code: None,
        healthy: false,
        response_ms: None,
        checked_at: now_rfc3339(),
        error: Some(msg.to_string()),
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
