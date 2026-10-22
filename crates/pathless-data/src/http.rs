//! Polite HTTP client: global concurrency limit, descriptive User-Agent, timeouts and
//! retries with exponential backoff on 5xx / 429 / transport errors.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use crate::{Error, Result};

pub const USER_AGENT: &str = "PathLess/0.1 (RF path analysis)";

/// Response of a completed request (any HTTP status that was not retried).
pub struct HttpResponse {
    pub status: u16,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    permits: Arc<Semaphore>,
    max_attempts: u32,
}

impl Http {
    pub fn new(max_concurrency: usize) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|e| Error::Http(e.to_string()))?;
        Ok(Http { client, permits: Arc::new(Semaphore::new(max_concurrency.max(1))), max_attempts: 4 })
    }

    /// GET `url`. Retries (1 s, 2 s, 4 s backoff) on transport errors, timeouts, 429 and
    /// 5xx. Other statuses (including 4xx) are returned to the caller.
    pub async fn get(&self, url: &str) -> Result<HttpResponse> {
        let mut last_err = String::new();
        for attempt in 0..self.max_attempts {
            if attempt > 0 {
                let delay = Duration::from_millis(1000 << (attempt - 1).min(5));
                tracing::warn!(attempt, ?delay, %last_err, "retrying request");
                tokio::time::sleep(delay).await;
            }
            let _permit = self.permits.acquire().await.map_err(|e| Error::Http(e.to_string()))?;
            tracing::debug!(%url, attempt, "GET");
            let resp = match self.client.get(url).send().await {
                Ok(r) => r,
                Err(e) => {
                    last_err = format!("request failed: {e}");
                    continue;
                }
            };
            let status = resp.status().as_u16();
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            if status == 429 || status >= 500 {
                last_err = format!("HTTP {status}");
                continue;
            }
            match resp.bytes().await {
                Ok(b) => return Ok(HttpResponse { status, content_type, bytes: b.to_vec() }),
                Err(e) => {
                    last_err = format!("reading body failed: {e}");
                    continue;
                }
            }
        }
        Err(Error::Http(format!("{last_err} after {} attempts: {url}", self.max_attempts)))
    }
}
