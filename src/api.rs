use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer, Serialize};

pub const DEFAULT_API: &str = "https://inbox.pilvineuvos.com";

/// One row of `GET /api/inboxes/{inbox}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    #[serde(deserialize_with = "string_or_number")]
    pub id: String,
    #[serde(default)]
    pub sender: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    /// Epoch milliseconds.
    #[serde(deserialize_with = "millis")]
    pub received_at: i64,
}

pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
}

impl Client {
    pub fn new(base: &str) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("pn/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_string(),
        })
    }

    /// Messages in `inbox` from the last ~24h, newest first (max 100).
    pub fn list(&self, inbox: &str) -> Result<Vec<Summary>> {
        let url = format!("{}/api/inboxes/{}", self.base, encode_segment(inbox));
        let resp = self.get(&url)?;
        if !resp.status().is_success() {
            bail!("API returned {} for {url}", resp.status());
        }
        resp.json().context("API returned invalid JSON")
    }

    /// Raw RFC 822 bytes of a message.
    pub fn fetch_raw(&self, id: &str) -> Result<Vec<u8>> {
        let url = format!("{}/api/messages/{}", self.base, encode_segment(id));
        let resp = self.get(&url)?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("message {id} not found (it may have expired after ~24h, or the id is wrong)");
        }
        if !resp.status().is_success() {
            bail!("API returned {} for {url}", resp.status());
        }
        Ok(resp
            .bytes()
            .context("failed to read message body")?
            .to_vec())
    }

    fn get(&self, url: &str) -> Result<reqwest::blocking::Response> {
        self.http
            .get(url)
            .send()
            .with_context(|| format!("request to {url} failed"))
    }
}

fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn string_or_number<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        other => Err(serde::de::Error::custom(format!(
            "expected string or number id, got {other}"
        ))),
    }
}

fn millis<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .ok_or_else(|| serde::de::Error::custom("invalid received_at")),
        serde_json::Value::String(s) => s
            .parse()
            .map_err(|_| serde::de::Error::custom("invalid received_at")),
        other => Err(serde::de::Error::custom(format!(
            "expected numeric received_at, got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_accepts_string_and_numeric_ids() {
        let rows: Vec<Summary> = serde_json::from_str(
            r#"[{"id":"abc-1","sender":"a@b.c","subject":"Hi","received_at":1700000000000},
                {"id":42,"sender":null,"subject":null,"received_at":1700000000000.0}]"#,
        )
        .unwrap();
        assert_eq!(rows[0].id, "abc-1");
        assert_eq!(rows[1].id, "42");
        assert_eq!(rows[1].received_at, 1_700_000_000_000);
        assert!(rows[1].sender.is_none());
    }

    #[test]
    fn encodes_path_segments() {
        assert_eq!(encode_segment("foo-bar.1"), "foo-bar.1");
        assert_eq!(encode_segment("a/b c"), "a%2Fb%20c");
    }
}
