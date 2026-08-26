# ---------- 1 · Next.js static UI ----------
FROM node:22-alpine AS web
WORKDIR /web
COPY web/package.json web/package-lock.json* ./
RUN npm install --no-audit --no-fund
COPY web/ .
RUN npm run build          # STATIC_EXPORT=1 -> /web/out

# ---------- 2 · Rust API ----------
FROM rust:1-alpine AS api
WORKDIR /src
COPY server/Cargo.toml server/Cargo.lock* ./
COPY server/src ./src
RUN cargo build --release

# ---------- 3 · runtime (ffmpeg + yt-dlp + single static binary) ----------
FROM python:3.12-alpine
RUN apk add --no-cache ffmpeg ca-certificates \
    && pip install --no-cache-dir yt-dlp

COPY --from=api /src/target/release/kv-dl-server /usr/local/bin/kv-dl-server
COPY --from=web /web/out /app/public

WORKDIR /app
ENV PORT=8080 PUBLIC_DIR=/app/public
EXPOSE 8080
CMD ["kv-dl-server"]
