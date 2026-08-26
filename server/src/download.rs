//! Streaming download pipeline: ffmpeg merges/transcodes straight into the
//! HTTP response body. Nothing is written to the server's disk.

use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use bytes::Bytes;
use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStdout};
use tokio_stream::wrappers::ReceiverStream;

const CHUNK: usize = 256 * 1024;
const FIRST_CHUNK_TIMEOUT: Duration = Duration::from_secs(45);

struct Running {
    first: Bytes,
    out: ChildStdout,
    // kill_on_drop(true) is set on every child -> dropped here means reaped.
    _children: Vec<Child>,
}

pub enum Strategy {
    /// One process: ffmpeg reading HTTPS inputs directly, audio encode, or a
    /// `sh -c "yt-dlp | ffmpeg"` pipeline fallback.
    Single { prog: String, args: Vec<String> },
}

fn base_command(prog: &str) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(prog);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    c
}

async fn read_first(out: &mut ChildStdout) -> Option<Bytes> {
    let mut buf = vec![0u8; CHUNK];
    match tokio::time::timeout(FIRST_CHUNK_TIMEOUT, out.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => Some(Bytes::copy_from_slice(&buf[..n])),
        _ => None,
    }
}

async fn start(s: &Strategy) -> Option<Running> {
    match s {
        Strategy::Single { prog, args } => {
            let mut child = base_command(prog).args(args).spawn().ok()?;
            let mut out = child.stdout.take()?;
            let first = read_first(&mut out).await?;
            Some(Running { first, out, _children: vec![child] })
        }
    }
}

fn prog_name(s: &Strategy) -> &str {
    match s {
        Strategy::Single { prog, .. } => prog,
    }
}

/// Try each strategy until one produces data, then stream its stdout to the
/// client. The download counter fires exactly when streaming starts.
pub fn stream_response(
    strategies: Vec<Strategy>,
    mimetype: &'static str,
    content_disposition: String,
    on_start: impl FnOnce() + Send + 'static,
) -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(8);

    tokio::spawn(async move {
        for (i, s) in strategies.iter().enumerate() {
            let running = start(s).await;
            let mut running = match running {
                Some(r) => r,
                None => {
                    eprintln!("[kv-dl] download: strategy #{i} ({}) produced no output", prog_name(s));
                    continue;
                }
            };
            eprintln!("[kv-dl] download: streaming via strategy #{i} ({})", prog_name(s));
            on_start();
            if tx.send(Ok(running.first)).await.is_err() {
                return; // client gone; children die via drop
            }
            let mut buf = vec![0u8; CHUNK];
            loop {
                match running.out.read(&mut buf).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => {
                        let chunk = Bytes::copy_from_slice(&buf[..n]);
                        if tx.send(Ok(chunk)).await.is_err() {
                            return;
                        }
                    }
                }
            }
        }
        let _ = tx
            .send(Err(std::io::Error::other("all download strategies failed")))
            .await;
        eprintln!(
            "[kv-dl] download: all {} strategies failed",
            strategies.len()
        );
    });

    let body = Body::from_stream(ReceiverStream::new(rx));
    let mut resp = Response::new(body);
    *resp.status_mut() = StatusCode::OK;
    if let Ok(v) = HeaderValue::from_str(mimetype) {
        resp.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    resp.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    let xab = axum::http::HeaderName::from_static("x-accel-buffering");
    resp.headers_mut().insert(xab, HeaderValue::from_static("off"));
    if let Ok(v) = HeaderValue::from_str(&content_disposition) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    resp
}

// ---------------------------------------------------------------------------
// Strategy builders (ports of the Python command builders)
// ---------------------------------------------------------------------------

pub fn http_headers(fmts: &[&Value], cookie: &str) -> String {
    let mut ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/126.0 Safari/537.36"
        .to_string();
    for f in fmts {
        if let Some(h) = f.get("http_headers").and_then(Value::as_object) {
            if let Some(v) = h.get("User-Agent").and_then(Value::as_str) {
                ua = v.to_string();
            }
        }
    }
    let mut s = format!("User-Agent: {ua}\r\nReferer: https://www.youtube.com/\r\n");
    if !cookie.is_empty() {
        s.push_str(&format!("Cookie: {cookie}\r\n"));
    }
    s
}

fn copy_audio(acodec: &str) -> bool {
    let c = acodec.to_lowercase();
    ["mp4a", "aac", "mp3"].iter().any(|k| c.contains(k))
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Shell snippet: stream one YouTube format into a FIFO via yt-dlp.
/// yt-dlp's own HTTP stack gets full speed where ffmpeg's HTTPS protocol
/// gets throttled by Googlevideo; ffmpeg only reads local FIFOs here.
///
/// `timeout` bounds orphaned writers; FIFOs are tmpfs — no media touches disk.
fn fifo_writer(fid_selector: &str, yt_url: &str, fifo: &str) -> String {
    format!(
        "timeout -s KILL 7200 yt-dlp --quiet --no-warnings --no-part -f {} -o - {} > {}",
        sh_quote(fid_selector),
        sh_quote(yt_url),
        fifo,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn video_strategies(
    vurl: String,
    aurl: String,
    acodec: String,
    headers: String,
    yt_url: String,
    fid: String,
) -> Vec<Strategy> {
    let audio_mode = if copy_audio(&acodec) { "copy" } else { "aac" };
    // Audio selector mirrors the codec decision: m4a when we can copy,
    // anything when we re-encode.
    let audio_selector =
        if copy_audio(&acodec) { "bestaudio[ext=m4a]/bestaudio" } else { "bestaudio/best" };

    // ── primary: two yt-dlp writers → tmpfs FIFOs → ffmpeg merge ──────────
    let fifo_script = format!(
        "set -e\n\
         D=/tmp/kvdl.$$\n\
         mkfifo \"$D.v\" \"$D.a\"\n\
         cleanup() {{ rm -f \"$D.v\" \"$D.a\"; }}\n\
         trap cleanup EXIT INT TERM\n\
         {vw} &\n\
         {aw} &\n\
         exec ffmpeg -hide_banner -loglevel error -fflags +nobuffer \
-i \"$D.v\" -i \"$D.a\" -map 0:v:0 -map 1:a:0? \
-c:v copy -c:a {audio_mode} -b:a 160k \
-movflags frag_keyframe+empty_moov+default_base_moof -f mp4 pipe:1",
        vw = fifo_writer(&fid, &yt_url, "\"$D.v\""),
        aw = fifo_writer(audio_selector, &yt_url, "\"$D.a\""),
    );

    let primary_args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-fflags".into(),
        "+nobuffer".into(),
        "-headers".into(),
        headers.clone(),
        "-i".into(),
        vurl.clone(),
        "-headers".into(),
        headers.clone(),
        "-i".into(),
        aurl.clone(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "1:a:0?".into(),
        "-c:v".into(),
        "copy".into(),
        "-c:a".into(),
        audio_mode.into(),
        "-b:a".into(),
        "160k".into(),
        "-movflags".into(),
        "frag_keyframe+empty_moov+default_base_moof".into(),
        "-f".into(),
        "mp4".into(),
        "pipe:1".into(),
    ];
    let fallback_args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        "pipe:0".into(),
        "-headers".into(),
        headers,
        "-i".into(),
        aurl,
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "1:a:0?".into(),
        "-c:v".into(),
        "copy".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "160k".into(),
        "-movflags".into(),
        "frag_keyframe+empty_moov+default_base_moof".into(),
        "-f".into(),
        "mp4".into(),
        "pipe:1".into(),
    ];
    // Fallback: video bytes flow through yt-dlp's stdout into ffmpeg's stdin.
    let ytdlp_cmd = [
        "yt-dlp",
        "--quiet",
        "--no-warnings",
        "--no-part",
        "-f",
        &format!("{fid}+bestaudio/{fid}/best"),
        "-o",
        "-",
        &yt_url,
    ]
    .iter()
    .map(|s| sh_quote(s))
    .collect::<Vec<_>>()
    .join(" ");
    let ffmpeg_cmd = std::iter::once("ffmpeg")
        .chain(fallback_args.iter().map(String::as_str))
        .map(sh_quote)
        .collect::<Vec<_>>()
        .join(" ");
    vec![
        Strategy::Single { prog: "sh".into(), args: vec!["-c".into(), fifo_script] },
        Strategy::Single { prog: "ffmpeg".into(), args: primary_args },
        Strategy::Single {
            prog: "sh".into(),
            args: vec!["-c".into(), format!("{ytdlp_cmd} | {ffmpeg_cmd}")],
        },
    ]
}

pub fn audio_strategy(aurl: String, abr: &str, headers: String, yt_url: String) -> Vec<Strategy> {
    // primary: yt-dlp writer → tmpfs FIFO → ffmpeg MP3 encode
    let fifo_script = format!(
        "set -e\n\
         D=/tmp/kvdl.$$\n\
         mkfifo \"$D.a\"\n\
         cleanup() {{ rm -f \"$D.a\"; }}\n\
         trap cleanup EXIT INT TERM\n\
         {w} &\n\
         exec ffmpeg -hide_banner -loglevel error -i \"$D.a\" -vn \
-c:a libmp3lame -b:a {abr}k -f mp3 pipe:1",
        w = fifo_writer("bestaudio/best", &yt_url, "\"$D.a\""),
    );
    vec![
        Strategy::Single { prog: "sh".into(), args: vec!["-c".into(), fifo_script] },
        Strategy::Single {
            prog: "ffmpeg".into(),
            args: vec![
                "-hide_banner".into(),
                "-loglevel".into(),
                "error".into(),
                "-headers".into(),
                headers,
                "-i".into(),
                aurl,
                "-vn".into(),
                "-c:a".into(),
                "libmp3lame".into(),
                "-b:a".into(),
                format!("{abr}k"),
                "-f".into(),
                "mp3".into(),
                "pipe:1".into(),
            ],
        },
    ]
}
