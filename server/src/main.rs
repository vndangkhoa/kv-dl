//! KV-DL API server — metube-style YouTube downloader backend (Rust/Axum).
//!
//! Streams video+audio (ffmpeg-merged) or MP3 audio straight to the browser.
//! Nothing user-related is ever written to disk: uploaded cookies live in a
//! RAM-only vault behind HMAC-signed session cookies.

mod cookies;
mod download;
mod normalize;
mod ytdlp;

use axum::body::to_bytes;
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use percent_encoding::NON_ALPHANUMERIC;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tower_http::services::ServeDir;

const SID_COOKIE: &str = "kvdl_sid";
const SESSION_MAX_AGE: u64 = 12 * 3600;

struct AppState {
    secret: Vec<u8>,
    secure_cookies: bool,
    public_dir: String,
    vault: Mutex<cookies::Vault>,
    seen: Mutex<HashMap<String, Instant>>,
    downloads: AtomicU64,
}

type SharedState = Arc<AppState>;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// One-line request log for `docker logs` / `journalctl`.
async fn log_requests(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|q| q.to_string());
    let start = Instant::now();
    let resp = next.run(req).await;
    // Never print the download URL's signed params in full — keep it short.
    let target = match query.as_deref() {
        Some(q) if path == "/api/download" => format!("{path}?{}…", &q[..q.len().min(24)]),
        _ => path,
    };
    println!(
        "[kv-dl] {} {} -> {} ({} ms)",
        method,
        target,
        resp.status().as_u16(),
        start.elapsed().as_millis()
    );
    resp
}

fn random_secret() -> Vec<u8> {
    let a: u128 = rand::random();
    let b: u128 = rand::random();
    format!("{a:032x}{b:032x}").into_bytes()
}

fn set_cookie_value(state: &AppState, sid: &str, expire: bool) -> String {
    let mut c = format!("{SID_COOKIE}={sid}; Path=/; HttpOnly; SameSite=Lax");
    if expire {
        c.push_str("; Max-Age=0");
    } else {
        c.push_str(&format!("; Max-Age={SESSION_MAX_AGE}"));
        if state.secure_cookies {
            c.push_str("; Secure");
        }
    }
    c
}

/// Existing verified sid, if any.
fn current_sid(headers: &HeaderMap, state: &AppState) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    let value = cookies::extract_cookie(raw, SID_COOKIE)?;
    cookies::verify_sid(&state.secret, Some(value))
}

/// The domain this request was served on (port stripped) — used to derive the
/// domain-swap mirror suffix so any self-hosted domain works the same way.
fn self_host(headers: &HeaderMap) -> Option<String> {
    // Trust the proxy chain's original host first, then the direct Host.
    let forwarded = headers
        .get("x-forwarded-host")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let raw = match forwarded {
        Some(v) => v.to_string(),
        None => headers.get(header::HOST)?.to_str().ok()?.to_string(),
    };
    let host_no_port = match raw.rsplit_once(':') {
        // Don't mangle bare IPv6 hosts like "[::1]".
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => h,
        _ => raw.as_str(),
    };
    let host = host_no_port.trim_matches(['[', ']']).to_lowercase();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim_matches(|c| c == ' ' || c == '.');
    let cut = trimmed.chars().take(120).collect::<String>();
    if cut.is_empty() { "download".into() } else { cut }
}

fn content_disposition(filename: &str) -> String {
    let safe = sanitize_filename(filename);
    let ascii: String = safe.chars().filter(|c| c.is_ascii()).collect();
    let fallback = if ascii.is_empty() { "download" } else { &ascii };
    let encoded = percent_encoding::utf8_percent_encode(&safe, NON_ALPHANUMERIC);
    format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}

fn err_json(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(json!({ "error": msg.into() }))).into_response()
}

fn bump_downloads(state: &AppState) {
    let n = state.downloads.fetch_add(1, Ordering::Relaxed) + 1;
    if let Ok(path) = std::env::var("STATS_FILE") {
        if !path.is_empty() {
            let tmp = format!("{path}.tmp");
            if std::fs::write(&tmp, json!({ "total_downloads": n }).to_string()).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// handlers
// ---------------------------------------------------------------------------

async fn health() -> &'static str {
    "ok"
}

#[derive(Deserialize)]
struct InfoReq {
    url: String,
}

async fn api_info(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Option<Json<InfoReq>>,
) -> Response {
    let Some(Json(req)) = body else {
        return err_json(StatusCode::BAD_REQUEST, "Missing JSON body with a \"url\" field.");
    };
    let url = match normalize::normalize_url(&req.url, self_host(&headers).as_deref()) {
        Ok(u) => u,
        Err(e) => return err_json(StatusCode::BAD_REQUEST, e),
    };

    let cookie_text = current_sid(&headers, &state).and_then(|sid| {
        state.vault.lock().unwrap().get(&sid).map(|e| e.text)
    });

    let info = match ytdlp::extract_json(&url, cookie_text.as_deref()).await {
        Ok(v) => v,
        Err(e) => return err_json(StatusCode::BAD_GATEWAY, e.message),
    };
    let video = match ytdlp::to_video(info) {
        Ok(v) => v,
        Err(e) => return err_json(StatusCode::BAD_GATEWAY, e.message),
    };

    let duration = video.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
    let empty = vec![];
    let formats = video.get("formats").and_then(Value::as_array).unwrap_or(&empty);

    Json(json!({
        "normalized_url": url,
        "id": video.get("id"),
        "title": video.get("title"),
        "uploader": video.get("uploader").or_else(|| video.get("channel")),
        "duration_string": ytdlp::duration_string(duration),
        "thumbnail": video.get("thumbnail"),
        "webpage_url": video.get("webpage_url"),
        "video_options": ytdlp::build_video_options(formats, duration),
        "audio_bitrates": ytdlp::AUDIO_BITRATES,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct DownloadQuery {
    url: String,
    #[serde(default = "default_mode")]
    mode: String,
    fid: Option<String>,
    abr: Option<String>,
}

fn default_mode() -> String {
    "video".into()
}

async fn api_download(
    State(state): State<SharedState>,
    Query(q): Query<DownloadQuery>,
    headers: HeaderMap,
) -> Response {
    let url = match normalize::normalize_url(&q.url, self_host(&headers).as_deref()) {
        Ok(u) => u,
        Err(e) => return err_json(StatusCode::BAD_REQUEST, e),
    };

    let (cookie_text, cookie_hdr) = current_sid(&headers, &state)
        .and_then(|sid| state.vault.lock().unwrap().get(&sid))
        .map(|e| {
            let hdr = cookies::cookie_header(&e.text);
            (Some(e.text), hdr)
        })
        .unwrap_or((None, String::new()));

    let info = match ytdlp::extract_json(&url, cookie_text.as_deref()).await {
        Ok(v) => v,
        Err(e) => return err_json(StatusCode::BAD_GATEWAY, format!("yt-dlp failed: {}", e.message)),
    };
    let video = match ytdlp::to_video(info) {
        Ok(v) => v,
        Err(e) => return err_json(StatusCode::BAD_GATEWAY, e.message),
    };

    let title = sanitize_filename(video.get("title").and_then(Value::as_str).unwrap_or("youtube"));
    let empty = vec![];
    let formats = video.get("formats").and_then(Value::as_array).unwrap_or(&empty);

    let strategies;
    let mimetype;
    let filename;

    if q.mode == "audio" {
        let Some(audio) = ytdlp::pick_audio(formats) else {
            return err_json(StatusCode::BAD_GATEWAY, "No downloadable audio stream found.");
        };
        let abr = match q.abr.as_deref() {
            Some(a) if ytdlp::AUDIO_BITRATES.contains(&a) => a.to_string(),
            _ => "192".to_string(),
        };
        let Some(aurl) = audio.get("url").and_then(Value::as_str).map(str::to_string) else {
            return err_json(StatusCode::BAD_GATEWAY, "Audio stream has no URL.");
        };
        let hdrs = download::http_headers(&[audio], &cookie_hdr);
        strategies = download::audio_strategy(aurl, &abr, hdrs, url.clone());
        mimetype = "audio/mpeg";
        filename = format!("{title} [{abr}kbps].mp3");
    } else {
        let Some(fid) = q.fid.clone() else {
            return err_json(StatusCode::BAD_REQUEST, "Missing fid for video download.");
        };
        let Some(fmt) = ytdlp::find_format(formats, &fid) else {
            return err_json(StatusCode::BAD_REQUEST, "Unknown format id.");
        };
        let Some(audio) = ytdlp::pick_audio(formats) else {
            return err_json(StatusCode::BAD_GATEWAY, "No downloadable audio stream found.");
        };
        let (Some(vurl), Some(aurl)) = (
            fmt.get("url").and_then(Value::as_str).map(str::to_string),
            audio.get("url").and_then(Value::as_str).map(str::to_string),
        ) else {
            return err_json(StatusCode::BAD_GATEWAY, "Selected streams have no URL.");
        };
        let acodec = fmt.get("acodec").and_then(Value::as_str).unwrap_or("").to_string();
        let height = fmt.get("height").and_then(Value::as_i64).unwrap_or(0);
        let hdrs = download::http_headers(&[fmt, audio], &cookie_hdr);
        strategies = download::video_strategies(vurl, aurl, acodec, hdrs, url, fid);
        mimetype = "video/mp4";
        filename = format!("{title} [{height}p].mp4");
    }

    let cd = content_disposition(&filename);
    let st2 = Arc::clone(&state);
    Ok::<Response, std::convert::Infallible>(download::stream_response(
        strategies,
        mimetype,
        cd,
        move || bump_downloads(&st2),
    ))
    .into_response()
}

async fn api_stats(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    // Ensure every visitor has an identity so the online counter works.
    let (set_cookie, sid) = match current_sid(&headers, &state) {
        Some(sid) => (None, sid),
        None => {
            let (cookie_value, vault_key) = cookies::new_session(&state.secret);
            (Some(set_cookie_value(&state, &cookie_value, false)), vault_key)
        }
    };
    {
        let mut seen = state.seen.lock().unwrap();
        let now = Instant::now();
        seen.retain(|_, t| now.duration_since(*t).as_secs() < 300);
        seen.insert(sid, now);
    }
    let online = {
        let seen = state.seen.lock().unwrap();
        seen.values()
            .filter(|t| t.elapsed().as_secs() < 60)
            .count()
    };
    let body = json!({
        "online": online,
        "total_downloads": state.downloads.load(Ordering::Relaxed),
        "server_time": now_unix(),
    });
    match set_cookie {
        Some(c) => {
            let mut resp = Json(body).into_response();
            if let Ok(hv) = HeaderValue::from_str(&c) {
                resp.headers_mut().append(header::SET_COOKIE, hv);
            }
            resp
        }
        None => Json(body).into_response(),
    }
}

async fn api_cookies_status(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let entry = current_sid(&headers, &state)
        .and_then(|sid| state.vault.lock().unwrap().get(&sid));
    let server_default = std::env::var("COOKIES_FILE")
        .map(|p| std::path::Path::new(&p).exists())
        .unwrap_or(false);
    // Metadata only — cookie contents never leave the vault.
    Json(cookies::status_json(entry.as_ref(), server_default)).into_response()
}

/// POST /api/cookies/upload
/// Accepts either a multipart file upload (`file` field, any common format)
/// or a raw pasted body (`application/json` with {"text": …} or plain text).
async fn api_cookies_upload(
    State(state): State<SharedState>,
    headers: HeaderMap,
    req: Request,
) -> Response {
    let content_type =
        headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");

    let (text, source_name) = if content_type.starts_with("multipart/form-data") {
        let mut multipart = match Multipart::from_request(req, &()).await {
            Ok(m) => m,
            Err(e) => return err_json(StatusCode::BAD_REQUEST, e.body_text()),
        };
        let mut found: Option<(String, String)> = None; // (text, name)
        while let Ok(Some(field)) = multipart.next_field().await {
            if field.name() != Some("file") {
                continue;
            }
            let fname = field.file_name().unwrap_or("cookies.txt").to_string();
            let data = match field.bytes().await {
                Ok(d) => d,
                Err(e) => return err_json(StatusCode::BAD_REQUEST, format!("Upload failed: {e}")),
            };
            if data.len() > cookies::MAX_UPLOAD_BYTES {
                return err_json(StatusCode::BAD_REQUEST, "File too large (max 512 KB).");
            }
            let t = match std::str::from_utf8(&data) {
                Ok(t) => t.to_string(),
                Err(_) => {
                    return err_json(
                        StatusCode::BAD_REQUEST,
                        "Not valid UTF-8 text (is this really a cookies file?).",
                    )
                }
            };
            found = Some((t, fname));
            break;
        }
        match found {
            Some(x) => x,
            None => return err_json(
                StatusCode::BAD_REQUEST,
                "No file received. Choose a cookies file or paste the cookie text instead.",
            ),
        }
    } else {
        // Pasted text: JSON wrapper {"text": …} or the raw body itself.
        let body = match to_bytes(req.into_body(), cookies::MAX_UPLOAD_BYTES).await {
            Ok(b) => b,
            Err(_) => {
                return err_json(
                    StatusCode::BAD_REQUEST,
                    "Body too large or unreadable (max 512 KB).",
                )
            }
        };
        if body.is_empty() {
            return err_json(StatusCode::BAD_REQUEST, "No cookies provided.");
        }
        let t = match std::str::from_utf8(&body) {
            Ok(t) => t.trim().to_string(),
            Err(_) => return err_json(StatusCode::BAD_REQUEST, "Pasted cookies must be text."),
        };
        // Unwrap a JSON envelope if one was sent.
        let t = if (t.starts_with('{') || t.starts_with('[')) && content_type.contains("json") {
            match serde_json::from_str::<Value>(&t) {
                Ok(Value::Object(map)) if map.contains_key("text") => match map["text"].as_str() {
                    Some(s) => s.to_string(),
                    None => t,
                },
                _ => t,
            }
        } else {
            t
        };
        (t, "pasted-cookies".to_string())
    };

    // Normalize whatever format arrived into Netscape for yt-dlp.
    let norm = match cookies::normalize_any(&text) {
        Ok(n) => n,
        Err(e) => return err_json(StatusCode::BAD_REQUEST, e),
    };
    if norm.youtube == 0 {
        return err_json(
            StatusCode::BAD_REQUEST,
            "No YouTube cookies found — export/copy them while visiting youtube.com.",
        );
    }

    // Rotate identity + store in the RAM-only vault (keyed by payload).
    let (cookie_value, vault_key) = cookies::new_session(&state.secret);
    let old_sid = current_sid(&headers, &state);
    let entry = cookies::new_entry(
        norm.text,
        sanitize_filename(&source_name),
        norm.format.to_string(),
        norm.total,
    );
    {
        let mut vault = state.vault.lock().unwrap();
        if let Some(old) = old_sid {
            vault.remove(&old);
        }
        vault.put(vault_key, entry.clone());
    }

    let resp_body = json!({
        "active": true,
        "name": entry.name,
        "format": entry.format,
        "cookies": entry.count,
        "added_at": entry.added_unix,
    });
    let mut resp = Json(resp_body).into_response();
    let c = set_cookie_value(&state, &cookie_value, false);
    if let Ok(hv) = HeaderValue::from_str(&c) {
        resp.headers_mut().append(header::SET_COOKIE, hv);
    }
    resp
}

async fn api_cookies_clear(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    if let Some(sid) = current_sid(&headers, &state) {
        state.vault.lock().unwrap().remove(&sid);
    }
    Json(json!({ "active": false })).into_response()
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let secret = std::env::var("SECRET_KEY")
        .map(|s| s.into_bytes())
        .unwrap_or_else(|_| random_secret());

    let mut initial_downloads = 0u64;
    if let Ok(path) = std::env::var("STATS_FILE") {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(v) = serde_json::from_str::<Value>(&data) {
                initial_downloads = v["total_downloads"].as_u64().unwrap_or(0);
            }
        }
    }

    let state = Arc::new(AppState {
        secret,
        secure_cookies: std::env::var("SECURE_COOKIES").as_deref() == Ok("1"),
        public_dir: std::env::var("PUBLIC_DIR").unwrap_or_else(|_| "public".to_string()),
        vault: Mutex::new(cookies::Vault::new()),
        seen: Mutex::new(HashMap::new()),        downloads: AtomicU64::new(initial_downloads),
    });

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/info", post(api_info))
        .route("/api/download", get(api_download))
        .route("/api/stats", get(api_stats))
        .route("/api/cookies/upload", post(api_cookies_upload))
        .route("/api/cookies/status", get(api_cookies_status))
        .route("/api/cookies/clear", post(api_cookies_clear))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .fallback_service(ServeDir::new(&state.public_dir).append_index_html_on_directories(true))
        .layer(middleware::from_fn(log_requests))
        .with_state(state);

    ytdlp::init_js_runtime().await;

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind port");
    println!("KV-DL API (rust) listening on http://0.0.0.0:{port}");
    axum::serve(listener, app).await.expect("server error");
}
