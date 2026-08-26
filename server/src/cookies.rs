//! Per-user cookie vault (RAM ONLY) + Netscape cookies.txt parsing.

use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const COOKIE_TTL_SECS: u64 = 8 * 3600;
pub const MAX_UPLOAD_BYTES: usize = 512 * 1024;

#[derive(Clone)]
pub struct VaultEntry {
    pub text: String,
    pub name: String,
    pub count: usize,
    pub added_unix: u64,
    pub last_used: Instant,
}

/// Parse a Netscape cookies.txt file.
/// Returns (total_cookies, youtube_cookies).
pub fn parse_netscape(text: &str) -> (usize, usize) {
    let mut total = 0usize;
    let mut yt = 0usize;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let line = if let Some(rest) = line.strip_prefix("#HttpOnly_") {
            rest
        } else if line.starts_with('#') {
            continue;
        } else {
            line
        };
        let parts: Vec<&str> = if line.contains('\t') {
            line.split('\t').collect()
        } else {
            line.split_whitespace().collect()
        };
        if parts.len() != 7 {
            continue;
        }
        let domain = parts[0];
        if domain.is_empty() || parts[5].is_empty() {
            continue;
        }
        total += 1;
        let host = domain.trim_start_matches('.').to_lowercase();
        if host == "youtube.com" || host.ends_with(".youtube.com") || host == "youtu.be" {
            yt += 1;
        }
    }
    (total, yt)
}

/// Build a "name=value; name=value" Cookie header from a cookies.txt text.
pub fn cookie_header(text: &str) -> String {
    let mut pairs = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let line = if let Some(rest) = line.strip_prefix("#HttpOnly_") {
            rest
        } else if line.starts_with('#') {
            continue
                ;
        } else {
            line
        };
        let parts: Vec<&str> = if line.contains('\t') {
            line.split('\t').collect()
        } else {
            line.split_whitespace().collect()
        };
        if parts.len() == 7 && !parts[5].is_empty() {
            pairs.push(format!("{}={}", parts[5], parts[6]));
        }
    }
    pairs.join("; ")
}

// ---------------------------------------------------------------------------
// Signed session ids: "<payload>.<hex(hmac_sha256(secret,payload))>"
// ---------------------------------------------------------------------------

fn new_payload() -> String {
    let a: u128 = rand::random();
    let b: u128 = rand::random();
    format!("{a:032x}{b:032x}")
}

fn sign(secret: &[u8], payload: &str) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("hmac key");
    mac.update(payload.as_bytes());
    let tag = mac.finalize().into_bytes();
    tag.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn make_sid(secret: &[u8]) -> String {
    let payload = new_payload();
    let sig = sign(secret, &payload);
    format!("{payload}.{sig}")
}

/// New session identity: returns (cookie_value, vault_key).
/// The cookie carries the signed value; the vault is keyed by payload only
/// (what `verify_sid` hands back on later requests).
pub fn new_session(secret: &[u8]) -> (String, String) {
    let payload = new_payload();
    let sig = sign(secret, &payload);
    (format!("{payload}.{sig}"), payload)
}

pub fn verify_sid(secret: &[u8], value: Option<&str>) -> Option<String> {
    let value = value?;
    let (payload, sig) = value.rsplit_once('.')?;
    if payload.is_empty() || sig.is_empty() {
        return None;
    }
    // constant-time-ish compare via re-sign + exact match on fixed-size hex
    let expect = sign(secret, payload);
    if expect.len() != sig.len() {
        return None;
    }
    let mut diff = 0u8;
    for (a, b) in expect.bytes().zip(sig.bytes()) {
        diff |= a ^ b;
    }
    (diff == 0).then(|| payload.to_string())
}

pub fn extract_cookie<'a>(cookie_header_value: &'a str, name: &str) -> Option<&'a str> {
    for pair in cookie_header_value.split(';') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            if k.trim() == name {
                return Some(v.trim());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Vault operations (caller holds the lock)
// ---------------------------------------------------------------------------

pub struct Vault {
    pub map: HashMap<String, VaultEntry>,
}

impl Vault {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    /// Fetch + touch; sweeps expired entries lazily.
    pub fn get(&mut self, sid: &str) -> Option<VaultEntry> {
        let now = Instant::now();
        self.map.retain(|_, e| now.duration_since(e.last_used).as_secs() < COOKIE_TTL_SECS);
        self.map.get_mut(sid).map(|e| {
            e.last_used = now;
            e.clone()
        })
    }

    pub fn put(&mut self, sid: String, entry: VaultEntry) {
        self.map.insert(sid, entry);
    }

    pub fn remove(&mut self, sid: &str) -> bool {
        self.map.remove(sid).is_some()
    }
}

pub fn new_entry(text: String, name: String, count: usize) -> VaultEntry {
    VaultEntry {
        text,
        name,
        count,
        added_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        last_used: Instant::now(),
    }
}

pub fn status_json(entry: Option<&VaultEntry>, server_default: bool) -> serde_json::Value {
    match entry {
        Some(e) => json!({
            "active": true,
            "name": e.name,
            "cookies": e.count,
            "added_at": e.added_unix,
        }),
        None => json!({ "active": false, "server_default": server_default }),
    }
}
