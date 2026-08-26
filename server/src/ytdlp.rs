//! yt-dlp invocation + format selection (ports of the Python logic).

use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

pub const AUDIO_BITRATES: [&str; 3] = ["128", "192", "320"];
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub struct ExtractError {
    pub message: String,
}

impl ExtractError {
    fn new(msg: impl Into<String>) -> Self {
        Self { message: msg.into() }
    }
}

/// Detect a JS runtime once at startup. yt-dlp needs one (node/deno) for
/// modern YouTube extraction — logged-in/cookie sessions otherwise often
/// return no usable formats ("Requested format is not available").
static JS_ARGS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();

pub async fn init_js_runtime() {
    let ok = tokio::process::Command::new("node")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = JS_ARGS.set(if ok { vec!["--js-runtimes", "node"] } else { Vec::new() });
    eprintln!(
        "[kv-dl] yt-dlp JS runtime: {}",
        if ok { "node" } else { "none (extraction may be limited)" }
    );
}

pub fn js_args() -> &'static [&'static str] {
    JS_ARGS.get().map(|v| v.as_slice()).unwrap_or(&[])
}

/// Run `yt-dlp --dump-single-json` for one video. Cookies, when present, are
/// piped through stdin as `--cookies /dev/stdin` so nothing touches the disk.
///
/// Logged-in sessions sometimes negotiate clients whose format list can't be
/// selected ("Requested format is not available"). On such failures we fall
/// back progressively: default client override, then a cookie-less attempt.
pub async fn extract_json(url: &str, cookies_text: Option<&str>) -> Result<Value, ExtractError> {
    let mut result = extract_json_once(url, cookies_text, &[]).await;
    if let Err(e) = &result {
        if should_retry(&e.message) && cookies_text.is_some() {
            eprintln!("[kv-dl] yt-dlp: cookie'd extraction failed, retrying with default client");
            result =
                extract_json_once(url, cookies_text, &["--extractor-args", "youtube:player_client=default"]).await;
        }
    }
    if let Err(e) = &result {
        if should_retry(&e.message) && cookies_text.is_some() {
            eprintln!("[kv-dl] yt-dlp: still failing, retrying WITHOUT cookies");
            result =
                extract_json_once(url, None, &["--extractor-args", "youtube:player_client=default"]).await;
        }
    }
    result
}

fn should_retry(msg: &str) -> bool {
    msg.contains("Requested format")
        || msg.contains("Unable to download")
        || msg.contains("unable to download")
        || msg.contains("Precondition check failed")
        || msg.contains("Sign in to confirm")
}

async fn extract_json_once(
    url: &str,
    cookies_text: Option<&str>,
    extra_args: &[&str],
) -> Result<Value, ExtractError> {
    let mut cmd = tokio::process::Command::new("yt-dlp");
    cmd.args([
        "--dump-single-json",
        "--no-playlist",
        "--no-warnings",
        "--socket-timeout",
        "20",
        "--retries",
        "2",
    ]);
    cmd.args(js_args());
    cmd.args(extra_args);
    if cookies_text.is_some() {
        cmd.arg("--cookies").arg("/dev/stdin");
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    // Optional server-wide fallback file (documented env var).
    if cookies_text.is_none() {
        if let Ok(path) = std::env::var("COOKIES_FILE") {
            if std::path::Path::new(&path).exists() {
                cmd.arg("--cookies").arg(path);
            }
        }
    }
    cmd.arg(url);

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| ExtractError::new(format!("failed to spawn yt-dlp: {e}")))?;

    if let Some(text) = cookies_text {
        let mut stdin = child.stdin.take();
        if let Some(s) = stdin.as_mut() {
            let _ = s.write_all(text.as_bytes()).await;
            let _ = s.shutdown().await;
        }
        drop(stdin);
    }

    let out = tokio::time::timeout(Duration::from_secs(90), child.wait_with_output())
        .await
        .map_err(|_| ExtractError::new("yt-dlp timed out"))?
        .map_err(|e| ExtractError::new(format!("yt-dlp failed: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("extraction failed");
        let mut msg = last.to_string();
        let lower = stderr.to_lowercase();
        if lower.contains("sign in") || lower.contains("bot") || lower.contains("cookies") {
            msg.push_str(" (YouTube may require login — upload your cookies in the webapp.)");
        }
        return Err(ExtractError::new(msg));
    }

    if out.stdout.len() > MAX_OUTPUT_BYTES {
        return Err(ExtractError::new("yt-dlp output too large"));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| ExtractError::new(format!("bad JSON from yt-dlp: {e}")))
}

/// Normalize a playlist/live result to a single video object.
pub fn to_video(mut info: Value) -> Result<Value, ExtractError> {
    if info.get("is_live").and_then(Value::as_bool) == Some(true) {
        return Err(ExtractError::new("Live streams are not supported."));
    }
    if info.get("_type").and_then(Value::as_str) == Some("playlist") {
        let first = info
            .get_mut("entries")
            .and_then(Value::as_array_mut)
            .and_then(|a| if a.is_empty() { None } else { Some(a.remove(0)) });
        match first {
            Some(v) => info = v,
            None => return Err(ExtractError::new("Empty playlist result.")),
        }
    }
    Ok(info)
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn fmt_size(f: &Value, duration: f64) -> f64 {
    if let Some(s) = f.get("filesize").and_then(Value::as_f64) {
        return s;
    }
    if let Some(s) = f.get("filesize_approx").and_then(Value::as_f64) {
        return s;
    }
    let tbr = num(f, "tbr").max(num(f, "abr"));
    if tbr > 0.0 && duration > 0.0 {
        return tbr * 1000.0 / 8.0 * duration;
    }
    0.0
}

/// One entry per height (highest bitrate / h264 preferred), sorted desc.
pub fn build_video_options(formats: &[Value], duration: f64) -> Vec<Value> {
    use std::collections::HashMap;

    let best_audio_size = formats
        .iter()
        .filter(|f| is_audio_only(f))
        .map(|f| fmt_size(f, duration))
        .fold(0.0f64, f64::max);

    #[derive(Clone)]
    struct Pick {
        score: (f64, i32, i32, i32),
        fid: String,
        fps: i32,
        size_mb: Option<u64>,
    }

    let mut by_height: HashMap<i64, Pick> = HashMap::new();

    use std::collections::hash_map::Entry;
    for f in formats {
        let vcodec = f.get("vcodec").and_then(Value::as_str).unwrap_or("none");
        let protocol = f.get("protocol").and_then(Value::as_str).unwrap_or("");
        let height = match f.get("height").and_then(Value::as_i64) {
            Some(h) if h >= 144 => h,
            _ => continue,
        };
        if vcodec == "none" || !(protocol == "https" || protocol == "http") {
            continue;
        }
        let tbr = num(f, "tbr");
        let avc = i32::from(vcodec.starts_with("avc"));
        let mp4 = i32::from(f.get("ext").and_then(Value::as_str) == Some("mp4"));
        let fps = num(f, "fps") as i32;
        let score = (tbr, avc, mp4, fps);

        let candidate = Pick {
            score,
            fid: f.get("format_id").and_then(Value::as_str).unwrap_or("").to_string(),
            fps,
            size_mb: {
                let total = fmt_size(f, duration) + best_audio_size;
                if total > 0.0 { Some((total / 1_048_576.0).round() as u64) } else { None }
            },
        };

        match by_height.entry(height) {
            Entry::Vacant(v) => {
                v.insert(candidate);
            }
            Entry::Occupied(mut o) => {
                if candidate.score > o.get().score {
                    o.insert(candidate);
                }
            }
        }
    }

    let mut picks: Vec<(i64, Pick)> = by_height.into_iter().collect();
    picks.sort_by(|a, b| b.0.cmp(&a.0));
    picks.truncate(10);

    picks
        .into_iter()
        .map(|(height, p)| {
            let label = if p.fps > 30 { format!("{height}p{}", p.fps) } else { format!("{height}p") };
            json!({
                "fid": p.fid,
                "label": label,
                "height": height,
                "size_mb": p.size_mb,
            })
        })
        .collect()
}

fn is_audio_only(f: &Value) -> bool {
    let acodec = f.get("acodec").and_then(Value::as_str).unwrap_or("none");
    let vcodec = f.get("vcodec").and_then(Value::as_str).unwrap_or("none");
    acodec != "none" && vcodec == "none"
}

/// Best https audio stream (max abr then tbr).
pub fn pick_audio<'a>(formats: &'a [Value]) -> Option<&'a Value> {
    formats
        .iter()
        .filter(|f| {
            let proto = f.get("protocol").and_then(Value::as_str).unwrap_or("");
            is_audio_only(f) && (proto == "https" || proto == "http")
        })
        .max_by(|a, b| {
            let ka = (num(a, "abr"), num(a, "tbr"));
            let kb = (num(b, "abr"), num(b, "tbr"));
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        })
}

pub fn find_format<'a>(formats: &'a [Value], fid: &str) -> Option<&'a Value> {
    formats.iter().find(|f| f.get("format_id").and_then(Value::as_str) == Some(fid))
}

pub fn duration_string(secs: f64) -> String {
    if secs <= 0.0 {
        return String::new();
    }
    let total = secs.round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Fast playlist/channel listing: `--flat-playlist` returns entries without
/// extracting each video's formats. Capped via `--playlist-items`.
pub async fn extract_playlist(
    url: &str,
    cookies_text: Option<&str>,
    max_items: usize,
) -> Result<Value, ExtractError> {
    let mut cmd = tokio::process::Command::new("yt-dlp");
    cmd.args([
        "--flat-playlist",
        "--dump-single-json",
        "--no-warnings",
        "--socket-timeout",
        "20",
        "--retries",
        "2",
        "--playlist-items",
        &format!("1:{max_items}"),
    ]);
    cmd.args(js_args());
    if cookies_text.is_some() {
        cmd.arg("--cookies").arg("/dev/stdin");
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
        if let Ok(path) = std::env::var("COOKIES_FILE") {
            if std::path::Path::new(&path).exists() {
                cmd.arg("--cookies").arg(path);
            }
        }
    }
    cmd.arg(url);

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| ExtractError::new(format!("failed to spawn yt-dlp: {e}")))?;

    if let Some(text) = cookies_text {
        let mut stdin = child.stdin.take();
        if let Some(s) = stdin.as_mut() {
            let _ = s.write_all(text.as_bytes()).await;
            let _ = s.shutdown().await;
        }
        drop(stdin);
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| ExtractError::new(format!("yt-dlp failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.lines().filter(|l| l.starts_with("ERROR")).next().unwrap_or("yt-dlp failed");
        return Err(ExtractError::new(msg.to_string()));
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .map_err(|e| ExtractError::new(format!("Could not parse yt-dlp output: {e}")))
}
