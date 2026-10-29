//! The LAN LM naming engine: one OpenAI-compatible chat-completion request
//! against a private server, turned into a kebab-case slug. Native Rust (no
//! Python, no codex CLI): a single HTTPS call with the same fail-open contract
//! as the other engines — `None` on any failure, so the caller can fall back.
//!
//! Configuration lives in `lm.json` inside the per-plugin config dir
//! (`HERDR_PLUGIN_CONFIG_DIR`), next to the optional `branch-prefix` file:
//!
//! ```json
//! {
//!   "url": "https://lm.example.net/v1/chat/completions",
//!   "resolve": "lm.example.net:443:192.168.50.141",
//!   "model": "qwen3.8-flash-next-nvfp4",
//!   "timeout_seconds": 22
//! }
//! ```
//!
//! `resolve` is an optional DNS pin (`host:port:ip`): traffic for that host is
//! sent straight to the IP while TLS SNI and certificate validation stay on the
//! original hostname. `HERDR_NAMING_LM_URL` / `HERDR_NAMING_LM_MODEL` override
//! the corresponding fields from the environment.

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use ureq::AgentBuilder;

use crate::slug;

const PROMPT_LIMIT: usize = 2000;
const DEFAULT_TIMEOUT_SECS: u64 = 22;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const SYSTEM_PROMPT: &str = "Generate a concise English git branch slug from \
     the task. Return only 2-4 lowercase words joined by hyphens. No \
     explanation or reasoning.";

#[derive(Debug, Deserialize, PartialEq)]
pub struct Config {
    /// Full OpenAI-compatible chat-completions URL, e.g.
    /// `https://host/v1/chat/completions`.
    pub url: String,
    /// Optional DNS pin, `host:port:ip`, as in curl's `--resolve`.
    #[serde(default)]
    pub resolve: Option<String>,
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

/// Path of `lm.json` in the plugin config dir, if herdr told us where it is.
pub fn config_path() -> Option<PathBuf> {
    let dir = env::var("HERDR_PLUGIN_CONFIG_DIR").ok()?;
    Some(PathBuf::from(dir).join("lm.json"))
}

/// Load the LM config, applying the env overrides. `None` when the config dir
/// or file is missing or malformed — callers treat that as "engine disabled".
pub fn load() -> Option<Config> {
    let path = config_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    let mut config: Config = serde_json::from_str(&text).ok()?;
    if let Ok(url) = env::var("HERDR_NAMING_LM_URL") {
        if !url.is_empty() {
            config.url = url;
        }
    }
    if let Ok(model) = env::var("HERDR_NAMING_LM_MODEL") {
        if !model.is_empty() {
            config.model = model;
        }
    }
    if config.url.is_empty() || config.model.is_empty() {
        return None;
    }
    Some(config)
}

/// Ask the configured LAN LM for a slug for `prompt`. Fails open with `None`
/// on any config, network, schema, or content failure.
pub fn generate_slug(prompt: &str) -> Option<String> {
    let config = load()?;
    let truncated: String = prompt.chars().take(PROMPT_LIMIT).collect();

    let mut builder = AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout(Duration::from_secs(config.timeout_seconds.max(1)));
    if let Some(pin) = &config.resolve {
        builder = builder.resolver(parse_pin(pin)?);
    }
    let agent = builder.build();

    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": truncated},
        ],
        "max_tokens": 128,
        "temperature": 0.1,
        "stream": false,
        "chat_template_kwargs": {"enable_thinking": false},
    });

    let response: serde_json::Value = agent
        .post(&config.url)
        .send_json(body)
        .ok()?
        .into_json()
        .ok()?;

    let content = response
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let slug = slug::sanitize(content);
    if slug.is_empty() {
        None
    } else {
        Some(slug)
    }
}

/// A resolver that answers one pinned `host:port` with a fixed address and
/// nothing else (the LM server is the only host this engine talks to).
struct PinnedResolver {
    netloc: String,
    addr: SocketAddr,
}

impl ureq::Resolver for PinnedResolver {
    fn resolve(&self, netloc: &str) -> Result<Vec<SocketAddr>, std::io::Error> {
        if netloc.eq_ignore_ascii_case(&self.netloc) {
            Ok(vec![self.addr])
        } else {
            Ok(Vec::new())
        }
    }
}

/// Parse `host:port:ip` (the curl `--resolve` spelling, IPv4) into a resolver.
fn parse_pin(spec: &str) -> Option<PinnedResolver> {
    let (host_port, ip) = spec.rsplit_once(':')?;
    let (host, port) = host_port.split_once(':')?;
    if host.is_empty() || ip.is_empty() {
        return None;
    }
    let port: u16 = port.parse().ok()?;
    let addr: SocketAddr = format!("{ip}:{port}").parse().ok()?;
    Some(PinnedResolver {
        netloc: format!("{host}:{port}"),
        addr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ureq::Resolver;

    fn config() -> Config {
        Config {
            url: "https://lm.example.net/v1/chat/completions".to_string(),
            resolve: Some("lm.example.net:443:192.168.50.141".to_string()),
            model: "test-model".to_string(),
            timeout_seconds: 7,
        }
    }

    #[test]
    fn parses_full_config() {
        let text = r#"{
            "url": "https://lm.example.net/v1/chat/completions",
            "resolve": "lm.example.net:443:192.168.50.141",
            "model": "test-model",
            "timeout_seconds": 7
        }"#;
        assert_eq!(serde_json::from_str::<Config>(text).unwrap(), config());
    }

    #[test]
    fn resolve_and_timeout_are_optional() {
        let text = r#"{"url": "https://h/x", "model": "m"}"#;
        let parsed: Config = serde_json::from_str(text).unwrap();
        assert_eq!(parsed.timeout_seconds, DEFAULT_TIMEOUT_SECS);
        assert_eq!(parsed.resolve, None);
    }

    #[test]
    fn parses_pin_into_matching_resolver() {
        let pin = parse_pin("lm.example.net:443:192.168.50.141").unwrap();
        assert_eq!(pin.resolve("lm.example.net:443").unwrap().len(), 1);
        assert!(pin.resolve("lm.example.net:8443").unwrap().is_empty());
        assert!(pin.resolve("other.example.net:443").unwrap().is_empty());
    }

    #[test]
    fn rejects_malformed_pins() {
        assert!(parse_pin("lm.example.net:443").is_none());
        assert!(parse_pin("lm.example.net:notaport:1.2.3.4").is_none());
        assert!(parse_pin("lm.example.net:443:not-an-ip").is_none());
        assert!(parse_pin(":443:1.2.3.4").is_none());
    }
}
