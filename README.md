<div align="center">

# 📥 KV-DL

### Self-hosted YouTube downloader — Rust-fast, disk-free, one container

**Paste a link · preview it · pick a quality · stream video+audio or MP3 straight to your browser.**
Nothing ever touches the server's disk.

[![GitHub](https://img.shields.io/badge/GitHub-kv--dl-181717?logo=github)](https://github.com/vndangkhoa/kv-dl)
[![Forgejo](https://img.shields.io/badge/Forgejo-kv--dl-green?logo=git)](https://git.khoavo.myds.me/vndangkhoa/kv-dl)
[![Docker Hub](https://img.shields.io/docker/pulls/vndangkhoa/kv-dl?logo=docker&label=Docker%20Hub)](https://hub.docker.com/r/vndangkhoa/kv-dl)
[![GHCR](https://img.shields.io/badge/GHCR-vndangkhoa%2Fkv--dl-2496ED?logo=github)](https://ghcr.io/vndangkhoa/kv-dl)

`Rust · Axum` `Next.js · Tailwind v4` `yt-dlp · ffmpeg` `v1.1.5`

</div>

---

## ✨ Why KV-DL

| | |
|:---|:---|
| ⚡ **Throttle-proof pipeline** | ffmpeg fetching Googlevideo directly gets throttled below 1 MB/s. KV-DL routes each stream through yt-dlp's fast HTTP stack into kernel pipes (`FIFOs`) and lets ffmpeg *only* do the merging — full-speed downloads (~10–20 MB/s typical). |
| 💾 **Zero-disk by design** | Downloads are merged on the fly and piped straight into the HTTP response. Cookies live in a RAM-only vault behind HMAC-signed sessions. No temp files, nothing logged, nothing retained. |
| 🍪 **Cookies without fuss** | Paste them — no file needed. Netscape `cookies.txt`, JSON exports, `Cookie:` header strings and `Set-Cookie` lines are auto-detected and normalized. Age-restricted videos just work. |
| 🌐 **Any-domain friendly** | The `.com ⇄ your-domain` swap trick adapts itself to whatever domain hosts the instance (derived from `Host` / `X-Forwarded-Host`). Every self-hosted copy gets the same magic automatically. |
| 🎬 **Preview before you commit** | Click the thumbnail after fetching to play the real video inline (privacy-friendly `youtube-nocookie` embed) — confirm it's the right one before spending bandwidth. |
| 📊 **Live progress everywhere** | Elapsed-time indicator while YouTube is queried, byte-level progress bar while downloading (with Cancel), and a global download odometer + online-now counter on the page. |
| 🧯 **Self-healing extraction** | Ships a JS runtime for yt-dlp's modern clients, retries transient failures, falls back across player clients and cookie-less modes when logged-in sessions return broken format lists. |
| 📦 **One container, no Node runtime** | The UI compiles to a static bundle served by the Rust binary. ffmpeg + yt-dlp + node included. Runs on any amd64 host — VPS, NAS, Raspberry Pi-class hardware. |

---

## 🔁 Data flow

```mermaid
sequenceDiagram
    autonumber
    participant B as 🖥️ Browser
    participant A as ⚙️ Rust API (:8080)
    participant Y as 🐍 yt-dlp
    participant U as 📺 YouTube
    participant F as 🎞️ ffmpeg

    rect rgb(20, 30, 45)
    Note over B,U: ① Fetch — metadata
    B->>A: POST /api/info {url}
    A->>A: normalize URL<br/>(any-domain swap, strip playlist params)
    A->>Y: --dump-single-json (+ cookies from RAM vault)
    Y->>U: metadata & format queries
    U-->>Y: title, duration, formats
    Y-->>A: JSON
    A-->>B: id · title · thumbnail · qualities (+size est.)
    end

    rect rgb(18, 38, 32)
    Note over B,F: ② Download — stream merge
    B->>A: GET /api/download?url&mode&fid/abr
    A->>Y: spawn per-stream fetchers
    par video stream
        Y->>U: ranged HTTP (full speed)
        U-->>Y: video bytes
        Y-->>A: FIFO #1
    and audio stream
        Y->>U: ranged HTTP (full speed)
        U-->>Y: audio bytes
        Y-->>A: FIFO #2
    end
    A->>F: read FIFOs, mux (-c:v copy)
    F-->>B: fragmented MP4 / MP3, streamed chunks
    Note over B: live progress bar →<br/>save-to-disk (File System Access API)
    end
```

The same one-liner, minus the ceremony:

```text
yt-dlp ──video──▶ FIFO ─┐
                        ├──▶ ffmpeg ──▶ fragmented MP4 ──HTTP chunks──▶ 💾 browser
yt-dlp ──audio──▶ FIFO ─┘     (mux,     no Content-Length needed,
                          video is copied)   progress tracked client-side
```

If the primary path fails, the API transparently falls back to direct-URL ffmpeg,
then to a `yt-dlp | ffmpeg` pipe — whichever produces data first wins.

---

## 🚀 Quick start

**Development** (hot-reload UI + API):

```sh
cd server && PUBLIC_DIR=../web/out cargo run --release   # terminal 1
cd web && npm install && npm run dev                     # terminal 2 → http://localhost:3000
```

**Production-style single origin:**

```sh
(cd web && npm run build)                                # static export -> web/out
cd server && PUBLIC_DIR=../web/out cargo run --release   # → http://localhost:8080
```

## 🐳 Prebuilt images (recommended)

Multi-stage build: Node compiles the UI → Rust builds the API → final Alpine
runtime ships one binary + static UI + ffmpeg + yt-dlp + node.

```sh
docker run -d -p 8080:8080 vndangkhoa/kv-dl:latest                    # Docker Hub
docker run -d -p 8080:8080 ghcr.io/vndangkhoa/kv-dl:latest            # GHCR
docker run -d -p 8080:8080 git.khoavo.myds.me/vndangkhoa/kv-dl:latest # Forgejo
```

Or with Compose:

```sh
docker compose up --build -d      # build from source, http://localhost:8080
```

## 🖥️ Synology NAS

Use [`docker-compose.synology.yml`](docker-compose.synology.yml) with Container
Manager (DSM 7.2+): drop it into `/volume1/docker/kv-dl/`, create a Project from
that folder, done — it pulls the prebuilt image and persists stats under
`/volume1/docker/kv-dl/data`. Open `http://NAS-IP:8080`.

## 🌍 Deploy on your own domain

1. `docker compose up --build -d` on your VPS.
2. DNS `A` record → your IP.
3. HTTPS proxy — Caddy: `dl.example.net { reverse_proxy 127.0.0.1:8080 }`;
   nginx: keep `proxy_buffering off` so downloads stream through.
4. Set env: `SECRET_KEY`, `SECURE_COOKIES=1`, optional `STATS_FILE`, `COOKIES_FILE`.

Any domain works — the swap suffix derives from the serving host; links shaped
like `https://youtube.<your-domain>/watch?v=…` work out of the box.

---

## ⚙️ Configuration

| Variable         | Default | Description                                        |
|------------------|---------|----------------------------------------------------|
| `PORT`           | `8080`  | HTTP port                                          |
| `PUBLIC_DIR`     | `public`| Directory with the built Next.js static export     |
| `SECRET_KEY`     | random  | HMAC key for session cookies (set for restart-stable sessions) |
| `SECURE_COOKIES` | off     | `1` adds the `Secure` flag (use behind HTTPS)      |
| `STATS_FILE`     | –       | JSON file to persist the download counter          |
| `COOKIES_FILE`   | –       | Server-wide fallback `cookies.txt` when a user has none |

## 🔌 API

| Endpoint | Method | Purpose |
|---|---|---|
| `/api/info` | POST `{url}` | metadata + selectable formats (auto-retries transient failures) |
| `/api/download` | GET `?url=&mode=video\|audio&fid=&abr=` | streamed file (strategy chain, logs chosen path) |
| `/api/cookies/upload` \| `/status` \| `/clear` | POST/GET | RAM-only vault. Upload: multipart `file` **or** pasted body (`{"text": …}` / raw text); Netscape/JSON/header/Set-Cookie auto-detected |
| `/api/stats` | GET | `{online, total_downloads}` |
| `/api/health` | GET | liveness |

Every request is line-logged to stdout (`[kv-dl] METHOD path → status (ms)`),
so `docker logs` always tells you what happened.

## 🧱 Project layout

```
server/   Rust crate (axum): main.rs · normalize.rs · cookies.rs · ytdlp.rs · download.rs
web/      Next.js app: app/page.tsx · components/{Odometer,DemoTypewriter,CookiesPanel,SelfHostModal}.tsx
```

---

> [!WARNING]
> Downloading videos can violate YouTube's Terms of Service and content
> owners' rights. Use responsibly — prefer content you own or that is licensed
> for reuse.

<div align="center">
<sub>Inspired by metube · Powered by yt-dlp & ffmpeg · Built with Rust + Next.js</sub>
</div>
