# KV-DL — YouTube downloader (Rust + Next.js)

A self-hosted, metube-inspired YouTube downloader, fully rewritten:

- **Backend**: Rust (Axum) — URL normalization, yt-dlp orchestration, ffmpeg
  streaming, HMAC-signed sessions, RAM-only cookie vault, live stats.
- **Frontend**: Next.js + TypeScript (Tailwind v4) — built as a **static
  bundle and served by the Rust binary**. One container, one port, no Node
  runtime in production.

Paste a normal `youtube.com` link or the domain-swap mirror form
(`https://youtube.<hosting-domain>/watch?v=…`) — both work; playlist/radio
params are stripped automatically. Pick a quality (video+audio merged to MP4 by
ffmpeg) or MP3 (128/192/320 kbps). Files stream through memory straight to the
browser: **nothing is ever written to the server's disk.**

## Features

| | |
|---|---|
| Domain-swap trick | `.com` ⇄ `.hosting-domain` accepted everywhere, derived from each instance's own domain, animated demo in UI |
| Selectable quality | one entry per height up to 2160p, size estimates, h264-preferred for MP4 compat |
| Audio only | MP3 at 128/192/320 kbps |
| Streaming | ffmpeg pipes fragmented MP4 / MP3 directly into the HTTP response (with yt-dlp-piped fallback strategy) |
| Cookies vault | per-user upload of `cookies.txt` — RAM only, signed HttpOnly session cookie, never on disk/logged/returned, TTL + manual erase |
| Live stats | global download odometer + online-now counter (`/api/stats`, polled every 20 s), optional `STATS_FILE` persistence |
| Self-host guide | built-in modal with DNS + Caddy/nginx snippets |

## Layout

```
server/   Rust crate (axum): src/main.rs, normalize.rs, cookies.rs, ytdlp.rs, download.rs
web/      Next.js app: app/page.tsx, components/{Odometer,DemoTypewriter,CookiesPanel,SelfHostModal}.tsx
```

## Run locally

Requirements: Rust toolchain, Node 18+, plus `ffmpeg` and `yt-dlp` on PATH.

```sh
# terminal 1 — API (serves web/out if present)
cd server && PUBLIC_DIR=../web/out cargo run --release

# terminal 2 — hot-reload UI with API proxy
cd web && npm install && npm run dev      # http://localhost:3000
```

Production-style single origin:

```sh
(cd web && npm run build)                 # static export -> web/out
cd server && PUBLIC_DIR=../web/out cargo run --release   # http://localhost:8080
```

## Docker

```sh
docker compose up --build -d     # http://localhost:8080
```

Multi-stage image: node builds the UI → rust builds the API → final Alpine
runtime ships one binary + `web/out` + ffmpeg + yt-dlp.

## Deploy on your own domain

1. `docker compose up --build -d` on your VPS.
2. DNS `A` record → your IP.
3. HTTPS proxy — Caddy: `dl.example.net { reverse_proxy 127.0.0.1:8080 }`;
   nginx: keep `proxy_buffering off` so downloads stream through.
4. Set env: `SECRET_KEY`, `SECURE_COOKIES=1`, optional `STATS_FILE`,
   `COOKIES_FILE`.

Any domain works — the swap suffix is derived from whatever domain hosts the
instance (via its `Host` / `X-Forwarded-Host` header); `.vndns.net` is only
kept as a fallback for links already in circulation.

## Configuration

| Variable         | Default | Description                                        |
|------------------|---------|----------------------------------------------------|
| `PORT`           | `8080`  | HTTP port                                          |
| `PUBLIC_DIR`     | `public`| Directory with the built Next.js static export     |
| `SECRET_KEY`     | random  | HMAC key for session cookies (set for restart-stable sessions) |
| `SECURE_COOKIES` | off     | `1` adds the `Secure` flag (use behind HTTPS)      |
| `STATS_FILE`     | –       | JSON file to persist the download counter          |
| `COOKIES_FILE`   | –       | Server-wide fallback `cookies.txt` when a user has none |

## API

| Endpoint | Method | Purpose |
|---|---|---|
| `/api/info` | POST `{url}` | metadata + selectable formats |
| `/api/download?url=&mode=video\|audio&fid=&abr=` | GET | streamed file |
| `/api/cookies/upload` \| `/status` \| `/clear` | POST/GET | RAM-only vault |
| `/api/stats` | GET | `{online, total_downloads}` |
| `/api/health` | GET | liveness |

> Downloading videos can violate YouTube's ToS and content owners' rights.
> Use responsibly.
