//! Per-user cookie vault (RAM ONLY) + multi-format cookie parsing/normalizing.

use hmac::{Hmac, Mac};
use serde_json::{json, Map, Value};
use sha2::Sha256;
use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const COOKIE_TTL_SECS: u64 = 8 * 3600;
pub const MAX_UPLOAD_BYTES: usize = 512 * 1024;

#[derive(Clone)]
pub struct VaultEntry {
    pub text: String,
    pub name: String,
    pub format: String,
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
// Multi-format ingestion → normalized Netscape cookies.txt
// ---------------------------------------------------------------------------

/// Domain assumed when a format carries no domain info (Cookie header, maps).
const DEFAULT_DOMAIN: &str = ".youtube.com";

pub struct NormalizedCookies {
    /// "netscape" | "json" | "header" | "set-cookie"
    pub format: &'static str,
    /// Normalized Netscape cookies.txt content.
    pub text: String,
    pub total: usize,
    pub youtube: usize,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Strip quotes/control chars from cookie values.
fn clean_value(v: &str) -> String {
    v.trim().trim_matches('"').chars().filter(|c| !c.is_control()).collect()
}

fn netscape_line(
    domain: &str,
    path: &str,
    secure: bool,
    expires: i64,
    name: &str,
    value: &str,
) -> String {
    let d = domain.trim().to_lowercase();
    let (dom, sub) = match d.strip_prefix('.') {
        Some(rest) if !rest.is_empty() => (format!(".{rest}"), "TRUE"),
        _ => (d.clone(), "FALSE"),
    };
    let path = if path.trim().is_empty() { "/" } else { path.trim() };
    format!(
        "{dom}\t{sub}\t{path}\t{}\t{expires}\t{name}\t{value}",
        if secure { "TRUE" } else { "FALSE" }
    )
}

fn finish(lines: Vec<String>, format: &'static str) -> Result<NormalizedCookies, String> {
    if lines.is_empty() {
        return Err(match format {
            "json" => "No usable cookies found in the JSON input.".into(),
            "set-cookie" => "No usable cookies found in the Set-Cookie lines.".into(),
            _ => "No cookies recognized. Supported: Netscape cookies.txt, JSON export, \
                  Cookie: header string, or Set-Cookie lines."
                .into(),
        });
    }
    let text =
        format!("# Netscape HTTP Cookie File\n# normalized by KV-DL\n{}", lines.join("\n"));
    let (total, youtube) = parse_netscape(&text);
    Ok(NormalizedCookies { format, text, total, youtube })
}

/// One yt-dlp / browser-style JSON cookie object → Netscape line.
fn json_cookie_line(obj: &Map<String, Value>) -> Option<String> {
    let name = obj.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let value = match obj.get("value")? {
        Value::String(s) => clean_value(s),
        other => clean_value(&other.to_string()),
    };
    if value.is_empty() {
        return None;
    }
    let domain = obj.get("domain").and_then(Value::as_str).unwrap_or(DEFAULT_DOMAIN);
    let path = obj.get("path").and_then(Value::as_str).unwrap_or("/");
    let secure = obj.get("secure").and_then(Value::as_bool).unwrap_or(false);
    let expires = ["expirationDate", "expiryDate", "expires", "expiry"]
        .iter()
        .find_map(|k| obj.get(*k))
        .and_then(Value::as_f64)
        .map(|f| f as i64)
        .unwrap_or(0);
    Some(netscape_line(domain, path, secure, expires, name, &value))
}

/// `Cookie:` header / document.cookie text → Netscape lines (domain assumed).
fn header_pairs_to_netscape(raw: &str) -> Vec<String> {
    let joined = raw.lines().map(str::trim).collect::<Vec<_>>().join("; ");
    joined
        .split(';')
        .filter_map(|pair| {
            let pair = pair.trim();
            if pair.is_empty() || pair.starts_with('#') {
                return None;
            }
            let (name, value) = pair.split_once('=')?;
            let name = name.trim();
            // Skip attribute-looking leftovers (e.g. pasted Set-Cookie attrs).
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "path" | "domain" | "expires" | "max-age" | "secure" | "httponly" | "samesite"
            ) {
                return None;
            }
            if value.trim().is_empty() {
                return None;
            }
            Some(netscape_line(DEFAULT_DOMAIN, "/", true, 0, name, &clean_value(value)))
        })
        .collect()
}

/// One `Set-Cookie:` line → Netscape line. `Expires=` HTTP-dates are treated
/// as session cookies; `Max-Age` is honored when present.
fn set_cookie_line_to_netscape(line: &str) -> Option<String> {
    let s = line.trim();
    let s = s.strip_prefix("Set-Cookie:").or_else(|| s.strip_prefix("set-cookie:")).unwrap_or(s).trim();
    let mut parts = s.split(';');
    let (name, value) = parts.next()?.split_once('=')?;
    let name = name.trim();
    if name.is_empty() || value.trim().is_empty() {
        return None;
    }
    let mut domain = DEFAULT_DOMAIN.to_string();
    let mut path = "/".to_string();
    let mut expires = 0i64;
    for attr in parts {
        let attr = attr.trim();
        if let Some((k, v)) = attr.split_once('=') {
            match k.trim().to_ascii_lowercase().as_str() {
                "domain" => {
                    let v = v.trim().trim_start_matches('.');
                    if !v.is_empty() {
                        domain = format!(".{v}");
                    }
                }
                "path" => path = v.trim().to_string(),
                "max-age" => {
                    if let Ok(n) = v.trim().parse::<i64>() {
                        if n > 0 {
                            expires = now_unix() as i64 + n;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Some(netscape_line(&domain, &path, true, expires, name, &clean_value(value)))
}

fn looks_like_netscape(text: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim();
        let l = l.strip_prefix("#HttpOnly_").unwrap_or(l);
        if l.is_empty() || l.starts_with('#') {
            return false;
        }
        if l.contains('\t') && l.split('\t').count() == 7 {
            return true;
        }
        !l.contains(';') && l.split_whitespace().count() == 7
    })
}

/// Accept any common cookie representation and normalize it to Netscape:
/// - Netscape cookies.txt (exported by browser extensions)
/// - JSON array of cookie objects (yt-dlp style `{name,value,domain,…}`)
/// - JSON object mapping name → value (or name → cookie object)
/// - `Cookie:` request-header string / `document.cookie`
/// - one or more `Set-Cookie:` lines
pub fn normalize_any(raw: &str) -> Result<NormalizedCookies, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Empty cookie input.".into());
    }

    // --- JSON ---
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        let v: Value = serde_json::from_str(trimmed)
            .map_err(|_| "Input looked like JSON but could not be parsed.".to_string())?;
        let mut lines = Vec::new();
        match v {
            Value::Array(items) => {
                for item in items {
                    if let Value::Object(o) = item {
                        if let Some(l) = json_cookie_line(&o) {
                            lines.push(l);
                        }
                    }
                }
            }
            Value::Object(map) => {
                let mut simple = Vec::new();
                for (k, val) in &map {
                    match val {
                        Value::Object(o) => {
                            if let Some(l) = json_cookie_line(o) {
                                lines.push(l);
                            }
                        }
                        Value::String(s) if !k.trim().is_empty() && !s.trim().is_empty() => {
                            simple.push(netscape_line(
                                DEFAULT_DOMAIN,
                                "/",
                                true,
                                0,
                                k.trim(),
                                &clean_value(s),
                            ));
                        }
                        _ => {}
                    }
                }
                // Prefer full cookie objects; fall back to flat name→value pairs.
                if lines.is_empty() {
                    lines.extend(simple);
                }
            }
            _ => {}
        }
        return finish(lines, "json");
    }

    // --- Netscape ---
    if looks_like_netscape(trimmed) {
        return finish(
            trimmed.replace("\r\n", "\n").lines().map(str::to_string).collect(),
            "netscape",
        );
    }

    // --- Set-Cookie lines ---
    let data_lines: Vec<&str> =
        trimmed.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let set_cookie_count =
        data_lines.iter().filter(|l| l.to_ascii_lowercase().starts_with("set-cookie:")).count();
    if set_cookie_count > 0 && set_cookie_count * 2 >= data_lines.len() {
        let lines = data_lines.iter().filter_map(|l| set_cookie_line_to_netscape(l)).collect();
        return finish(lines, "set-cookie");
    }

    // --- Cookie header / document.cookie ---
    finish(header_pairs_to_netscape(trimmed), "header")
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

pub fn new_entry(text: String, name: String, format: String, count: usize) -> VaultEntry {
    VaultEntry {
        text,
        name,
        format,
        count,
        added_unix: now_unix(),
        last_used: Instant::now(),
    }
}

pub fn status_json(entry: Option<&VaultEntry>, server_default: bool) -> serde_json::Value {
    match entry {
        Some(e) => json!({
            "active": true,
            "name": e.name,
            "format": e.format,
            "cookies": e.count,
            "added_at": e.added_unix,
        }),
        None => json!({ "active": false, "server_default": server_default }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETSCAPE: &str = "# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t1999999999\tVISITOR_INFO1_LIVE\tabc\n.youtube.com\tTRUE\t/\tTRUE\t0\tSID\txyz\n#HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t1999999999\t__Secure-1PSID\tdef\n";

    #[test]
    fn netscape_passthrough_counts() {
        let n = normalize_any(NETSCAPE).unwrap();
        assert_eq!(n.format, "netscape");
        assert_eq!(n.total, 3);
        assert_eq!(n.youtube, 3);
    }

    #[test]
    fn cookie_header_string() {
        let n = normalize_any(
            "VISITOR_INFO1_LIVE=abc; SID=xyz; __Secure-1PSID=\"quoted v\"; Path=/",
        )
        .unwrap();
        assert_eq!(n.format, "header");
        assert_eq!(n.total, 3); // Path= attr skipped
        assert_eq!(n.youtube, 3);
        assert!(n.text.contains("VISITOR_INFO1_LIVE\t"));
        assert!(cookie_header(&n.text).contains("SID=xyz; __Secure-1PSID=quoted v"));
    }

    #[test]
    fn set_cookie_lines() {
        let n = normalize_any(
            "Set-Cookie: YSC=dVq_8h; Domain=.youtube.com; Path=/; HttpOnly; Secure\nSet-Cookie: GPS=1; Max-Age=300; Domain=.youtube.com",
        )
        .unwrap();
        assert_eq!(n.format, "set-cookie");
        assert_eq!(n.total, 2);
        assert!(n.text.contains("YSC"));
    }

    #[test]
    fn json_array_ytdlp_style() {
        let n = normalize_any(
            r#"[{"domain":".youtube.com","expirationDate":1893456000,"name":"SID","path":"/","value":"ja","secure":true},{"name":"NID","value":"nb"}]"#,
        )
        .unwrap();
        assert_eq!(n.format, "json");
        assert_eq!(n.total, 2);
        assert!(n.text.contains("1893456000\tSID\tja"));
        // missing domain defaults to youtube
        assert!(n.text.contains(".youtube.com\tTRUE\t/\tFALSE\t0\tNID\tnb"));
    }

    #[test]
    fn json_object_map() {
        let n =
            normalize_any(r#"{"SID":"abc","__Secure-1PSID":"def"}"#).unwrap();
        assert_eq!(n.format, "json");
        assert_eq!(n.total, 2);
        assert_eq!(n.youtube, 2);
    }

    #[test]
    fn garbage_rejected() {
        assert!(normalize_any("hello world").is_err());
        assert!(normalize_any("").is_err());
        assert!(normalize_any("{not json").is_err_and(|e| e.contains("JSON")));
        let n = normalize_any("foo=bar").unwrap(); // valid header pair, no YT domain issue
        assert_eq!(n.total, 1);
    }
}
