"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import type { InfoResponse, PlaylistEntry, PlaylistInfo, Stats, VideoOption } from "@/lib/types";
import { detectPlaylist } from "@/lib/playlist";
import Odometer from "@/components/Odometer";
import DemoTypewriter from "@/components/DemoTypewriter";
import CookiesPanel from "@/components/CookiesPanel";
import SelfHostModal from "@/components/SelfHostModal";
import PlaylistPanel, { type RowStatus } from "@/components/PlaylistPanel";

type Mode = "video" | "audio";

interface Writable2 {
  write: (c: Uint8Array) => Promise<void>;
  close: () => Promise<void>;
  abort: () => Promise<void>;
}
interface FileHandle2 {
  createWritable: () => Promise<Writable2>;
}
type DirHandle2 = {
  getFileHandle: (name: string, opts?: { create?: boolean }) => Promise<FileHandle2>;
};

interface DlState {
  label: string;
  received: number;
  total: number | null;
}

function mb(n: number) {
  return n >= 1024 * 1024 * 1024
    ? `${(n / 1024 ** 3).toFixed(2)} GB`
    : `${(n / 1024 ** 2).toFixed(1)} MB`;
}

function durationSec(s: string) {
  return s.split(":").reduce((a, b) => a * 60 + Number(b) || a, 0);
}

export default function Home() {
  const [url, setUrl] = useState("");
  const [loading, setLoading] = useState(false);
  const [fetchStart, setFetchStart] = useState<number | null>(null);
  const [fetchNow, setFetchNow] = useState<number>(0);
  const [error, setError] = useState("");
  const [info, setInfo] = useState<InfoResponse | null>(null);
  const [mode, setMode] = useState<Mode>("video");
  const [fid, setFid] = useState<string | null>(null);
  const [abr, setAbr] = useState("192");
  const [stats, setStats] = useState<Stats | null>(null);
  const [dl, setDl] = useState<DlState | null>(null);
  const dlCtrl = useRef<AbortController | null>(null);
  const [preview, setPreview] = useState(false);
  const [pl, setPl] = useState<PlaylistInfo | null>(null);
  const [plStatus, setPlStatus] = useState<Record<string, RowStatus | undefined>>({});
  const [batchBusy, setBatchBusy] = useState(false);
  const plCancel = useRef(false);
  const [streamToDisk, setStreamToDisk] = useState(false);

  // File System Access API lets us stream the download straight to disk
  // (Chrome/Edge); everywhere else we buffer to a Blob before saving.
  useEffect(() => {
    setStreamToDisk(typeof (window as unknown as { showSaveFilePicker?: unknown }).showSaveFilePicker === "function");
  }, []);

  const pollStats = useCallback(async () => {
    try {
      const res = await fetch("/api/stats");
      if (res.ok) setStats(await res.json());
    } catch {
      /* keep last values */
    }
  }, []);

  useEffect(() => {
    void pollStats();
    const id = setInterval(pollStats, 20000);
    return () => clearInterval(id);
  }, [pollStats]);

  // ticking timer while /api/info is in flight
  useEffect(() => {
    if (fetchStart === null) return;
    setFetchNow(0);
    const id = setInterval(() => setFetchNow((Date.now() - fetchStart) / 1000), 100);
    return () => clearInterval(id);
  }, [fetchStart]);

  // A shared/swapped link (?url=… or ?v=…) opens the app pre-filled.
  useEffect(() => {
    const sp = new URLSearchParams(window.location.search);
    const v = sp.get("v");
    const shared = sp.get("url") ?? (v ? `https://www.youtube.com/watch?v=${v}` : null);
    if (!shared) return;
    setUrl(shared);
    window.history.replaceState({}, "", window.location.pathname);
    void fetchInfo(undefined, shared);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function fetchPlaylist(target: string) {
    setLoading(true);
    setFetchStart(Date.now());
    setError("");
    setInfo(null);
    try {
      const res = await fetch("/api/playlist", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ url: target }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? `HTTP ${res.status}`);
      setPl(data as PlaylistInfo);
      setPlStatus({});
    } catch (err) {
      setError("Failed to load playlist:\n" + (err as Error).message);
    } finally {
      setLoading(false);
      setFetchStart(null);
    }
  }

  async function fetchInfo(e?: React.FormEvent, override?: string) {
    e?.preventDefault();
    const target = (override ?? url).trim();
    if (!target) return;
    // Pure playlist / channel links open the list view automatically.
    if (detectPlaylist(target)) {
      await fetchPlaylist(target);
      return;
    }
    setLoading(true);
    setFetchStart(Date.now());
    setError("");
    setInfo(null);
    try {
      const res = await fetch("/api/info", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ url: target }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? `HTTP ${res.status}`);
      setPreview(false);
      setInfo(data as InfoResponse);
      // default pick: closest to 1080p
      const opts = (data as InfoResponse).video_options;
      if (opts.length) {
        const best = opts.reduce((a, b) =>
          Math.abs(a.height - 1080) < Math.abs(b.height - 1080) ? a : b
        );
        setFid(best.fid);
        setMode("video");
      }
    } catch (err) {
      setError("Failed to fetch video info:\n" + (err as Error).message);
    } finally {
      setLoading(false);
      setFetchStart(null);
    }
  }

  const selectedOption = info?.video_options.find((o) => o.fid === fid);

  function targetFilename() {
    if (!info) return "download";
    const safe = info.title.replace(/[\\/:*?"<>|]/g, "_").slice(0, 120);
    return mode === "audio" ? `${safe} [${abr}kbps].mp3` : `${safe} [${selectedOption?.label ?? ""}].mp4`;
  }

  async function download() {
    if (!info || dl) return;
    const params = new URLSearchParams({ url, mode });
    let estTotal: number | null = null;
    const dur = durationSec(info.duration_string ?? "");
    if (mode === "video") {
      if (!fid) return;
      params.set("fid", fid);
      if (selectedOption?.size_mb) estTotal = selectedOption.size_mb * 1024 * 1024;
    } else {
      params.set("abr", abr);
      if (dur) estTotal = Math.round(((Number(abr) * 1000) / 8) * dur * 1.1);
    }

    // Chrome/Edge: ask where to save first (user gesture), then stream
    // straight to disk. Others: buffer to a Blob and hand it to the browser.
    type Writable = { write: (c: Uint8Array) => Promise<void>; close: () => Promise<void>; abort: () => Promise<void> };
    type FileHandle = { createWritable: () => Promise<Writable> };
    const w = window as unknown as { showSaveFilePicker?: (o?: object) => Promise<FileHandle> };
    let handle: FileHandle | null = null;
    if (typeof w.showSaveFilePicker === "function") {
      try {
        handle = await w.showSaveFilePicker({ suggestedName: targetFilename() });
      } catch {
        return; // user closed the save dialog — nothing to do
      }
    }

    const label = targetFilename();
    const ctrl = new AbortController();
    dlCtrl.current = ctrl;
    setDl({ label, received: 0, total: estTotal });
    setError("");

    let writable: Writable | null = null;
    try {
      const res = await fetch("/api/download?" + params.toString(), { signal: ctrl.signal });
      if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`);
      if (handle) writable = await handle.createWritable();

      const reader = res.body.getReader();
      const chunks: Uint8Array[] = [];
      let received = 0;
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        received += value.byteLength;
        setDl((s) => (s ? { ...s, received } : s));
        if (writable) await writable.write(value);
        else chunks.push(value);
      }

      if (writable) {
        await writable.close();
      } else {
        const blobUrl = URL.createObjectURL(new Blob(chunks as BlobPart[]));
        const a = document.createElement("a");
        a.href = blobUrl;
        a.download = label;
        document.body.appendChild(a);
        a.click();
        a.remove();
        setTimeout(() => URL.revokeObjectURL(blobUrl), 30000);
      }
    } catch (err) {
      if ((err as Error).name !== "AbortError") {
        setError("Download failed:\n" + (err as Error).message);
      }
      if (writable) await writable.abort().catch(() => {});
    } finally {
      dlCtrl.current = null;
      setDl(null);
    }
  }

  function pickEntry(e: PlaylistEntry) {
    setUrl(e.url);
    setPreview(false);
    void fetchInfo(undefined, e.url);
  }

  function loadPlaylistHint() {
    if (!info?.playlist_id) return;
    const id = info.playlist_id;
    // Radio mixes (RD…) only resolve from the watch-page context.
    const target =
      id.startsWith("RD") && info.normalized_url
        ? `${info.normalized_url}${info.normalized_url.includes("?") ? "&" : "?"}list=${id}`
        : `https://www.youtube.com/playlist?list=${id}`;
    void fetchPlaylist(target);
  }

  /** One video of a batch: info → best quality → stream to disk/blob. */
  async function downloadDirect(target: string, dir: DirHandle2 | null) {
    const res0 = await fetch("/api/info", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url: target }),
    });
    const meta = await res0.json();
    if (!res0.ok) throw new Error(meta.error ?? `HTTP ${res0.status}`);
    const opts = (meta.video_options ?? []) as VideoOption[];
    if (!opts.length) throw new Error("no downloadable formats");
    const best = opts.reduce((a, b) =>
      Math.abs(a.height - 1080) < Math.abs(b.height - 1080) ? a : b
    );
    const safe = String(meta.title ?? "video")
      .replace(/[\\/:*?"<>|]/g, "_")
      .slice(0, 120);
    const fallbackName = `${safe} [${best.label}].mp4`;

    const ctrl = new AbortController();
    dlCtrl.current = ctrl;
    const params = new URLSearchParams({ url: target, mode: "video", fid: best.fid });
    const res = await fetch("/api/download?" + params.toString(), { signal: ctrl.signal });
    if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`);

    const cd = res.headers.get("content-disposition") ?? "";
    const mStar = /filename\*=UTF-8''([^;]+)/i.exec(cd);
    const mPlain = /filename="([^"]+)"/i.exec(cd);
    const name = mStar ? decodeURIComponent(mStar[1]) : mPlain ? mPlain[1] : fallbackName;

    if (dir) {
      const fh = await dir.getFileHandle(name, { create: true });
      const ws = await fh.createWritable();
      const reader = res.body.getReader();
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        await ws.write(value);
      }
      await ws.close();
    } else {
      const chunks: Uint8Array[] = [];
      const reader = res.body.getReader();
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        chunks.push(value);
      }
      const blobUrl = URL.createObjectURL(new Blob(chunks as BlobPart[]));
      const a = document.createElement("a");
      a.href = blobUrl;
      a.download = name;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(blobUrl), 30000);
    }
  }

  async function downloadAll() {
    if (!pl || dl || batchBusy) return;
    const items = pl.entries.slice(0, 100);
    if (!items.length) return;
    const w = window as unknown as { showDirectoryPicker?: (o?: object) => Promise<DirHandle2> };
    let dir: DirHandle2 | null = null;
    if (typeof w.showDirectoryPicker === "function") {
      try {
        dir = await w.showDirectoryPicker({ mode: "readwrite" });
      } catch {
        return; // folder selection cancelled
      }
    }
    plCancel.current = false;
    setBatchBusy(true);
    try {
      for (const e of items) {
        if (plCancel.current) break;
        setPlStatus((s) => ({ ...s, [e.id]: "downloading" }));
        try {
          await downloadDirect(e.url, dir);
          setPlStatus((s) => ({ ...s, [e.id]: "done" }));
        } catch (err) {
          setPlStatus((s) => ({ ...s, [e.id]: "failed" }));
          if ((err as Error).name === "AbortError") break;
        }
      }
    } finally {
      setBatchBusy(false);
      dlCtrl.current = null;
    }
  }

  function cancelBatch() {
    plCancel.current = true;
    dlCtrl.current?.abort();
  }

  return (
    <main className="mx-auto max-w-xl px-4 pb-16 pt-12">
      <header className="text-center">
        <div className="text-3xl font-extrabold tracking-wide">
          KV<span className="text-cyan-300">-DL</span>
        </div>
        <h1 className="mt-1.5 text-[17px] font-semibold text-slate-200">
          Download YouTube video+audio — or audio only
        </h1>

        <DemoTypewriter />

        {/* live stats */}
        <div className="mt-5 flex flex-wrap justify-center gap-3">
          <div className="flex items-center gap-2.5 rounded-full border border-white/10 bg-white/[0.04] px-4 py-2">
            <span className="text-[11px] uppercase tracking-wider text-slate-400">↓ Downloads</span>
            <Odometer value={stats?.total_downloads ?? 0} />
          </div>
          <div className="flex items-center gap-2.5 rounded-full border border-white/10 bg-white/[0.04] px-4 py-2">
            <span className="live-dot h-2 w-2 rounded-full bg-red-500" />
            <span className="text-[11px] uppercase tracking-wider text-slate-400">Online now</span>
            <Odometer value={Math.max(1, stats?.online ?? 1)} className="!text-emerald-300" />
          </div>
        </div>
      </header>

      <section className="mt-7 rounded-2xl border border-white/10 bg-white/[0.045] p-5 shadow-xl backdrop-blur">
        <form onSubmit={fetchInfo} className="flex flex-col gap-2.5 sm:flex-row">
          <input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://youtube.com/watch?v=…"
            spellCheck={false}
            autoComplete="off"
            className="min-w-0 flex-1 rounded-xl border border-white/10 bg-black/35 px-4 py-3 text-[15px] outline-none transition-colors focus:border-cyan-300"
          />
          <button
            type="submit"
            disabled={loading}
            className="w-full rounded-xl bg-gradient-to-br from-cyan-300 to-emerald-300 px-6 py-3 font-bold text-[15px] text-teal-950 transition hover:brightness-110 disabled:opacity-55 sm:w-auto"
          >
            {loading ? "Fetching…" : "Fetch"}
          </button>
        </form>
        {loading && (
          <div className="mt-2.5 flex items-center justify-center gap-2 text-xs text-slate-400">
            <span className="live-dot h-1.5 w-1.5 rounded-full bg-cyan-300" />
            <span>Reading video info from YouTube… {fetchNow.toFixed(1)}s</span>
            {fetchNow > 6 && <span className="text-slate-500">(large pages / retries take longer)</span>}
          </div>
        )}
        {error && <p className="mt-3 whitespace-pre-wrap break-words text-sm text-rose-300">{error}</p>}

        {info && (
          <div className="mt-5">
            {info.id && preview ? (
              <div className="overflow-hidden rounded-xl border border-white/10 bg-black">
                <iframe
                  className="aspect-video w-full"
                  src={`https://www.youtube-nocookie.com/embed/${encodeURIComponent(info.id)}?autoplay=1&rel=0`}
                  title={info.title}
                  allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                  allowFullScreen
                />
                <button
                  type="button"
                  onClick={() => setPreview(false)}
                  className="w-full border-t border-white/10 bg-white/[0.03] py-1.5 text-[11.5px] text-slate-400 transition-colors hover:bg-white/[0.07] hover:text-cyan-200"
                >
                  ▴ Hide preview
                </button>
              </div>
            ) : (
              <div className="flex items-center gap-3.5">
                {info.thumbnail && (
                  <button
                    type="button"
                    onClick={() => setPreview(true)}
                    disabled={!info.id}
                    title={info.id ? "Click to preview the video" : undefined}
                    className="group relative shrink-0 overflow-hidden rounded-lg border border-white/10 focus:outline-none focus-visible:border-cyan-300 disabled:cursor-default"
                  >
                    {/* eslint-disable-next-line @next/next/no-img-element */}
                    <img
                      src={info.thumbnail}
                      alt=""
                      referrerPolicy="no-referrer"
                      className="aspect-video w-28 bg-black object-cover sm:w-40"
                    />
                    {info.id && (
                      <span className="absolute inset-0 grid place-items-center bg-black/30 opacity-0 transition-opacity group-hover:opacity-100">
                        <span className="grid h-10 w-10 place-items-center rounded-full bg-cyan-300 text-teal-950 shadow-lg transition-transform group-hover:scale-110">
                          ▶
                        </span>
                      </span>
                    )}
                    {info.duration_string && (
                      <span className="absolute bottom-1.5 right-1.5 rounded bg-black/75 px-1.5 py-0.5 text-[11px]">
                        {info.duration_string}
                      </span>
                    )}
                  </button>
                )}
                <div className="min-w-0">
                  <h2 className="line-clamp-3 text-[15px] leading-snug">{info.title}</h2>
                  {info.uploader && <p className="mt-1 text-xs text-slate-400">{info.uploader}</p>}
                  {info.id && (
                    <p className="mt-1.5 text-[11px] text-slate-500 group-hover:hidden">
                      Click the thumbnail to preview ▸
                    </p>
                  )}
                </div>
              </div>
            )}

            {/* mode switch */}
            <div className="mt-4 grid grid-cols-2 gap-1 rounded-xl border border-white/10 bg-black/30 p-1">
              {(["video", "audio"] as Mode[]).map((m) => (
                <button
                  key={m}
                  type="button"
                  onClick={() => setMode(m)}
                  className={`rounded-lg px-3 py-2 text-sm font-semibold transition-colors ${
                    mode === m ? "bg-cyan-300 text-teal-950" : "text-slate-400 hover:text-slate-200"
                  }`}
                >
                  {m === "video" ? "Video + sound (MP4)" : "Audio only (MP3)"}
                </button>
              ))}
            </div>

            {mode === "video" ? (
              <div className="mt-3 flex flex-wrap gap-2">
                {info.video_options.map((o) => (
                  <button
                    key={o.fid}
                    type="button"
                    onClick={() => setFid(o.fid)}
                    className={`relative rounded-full border px-3.5 py-2 text-[13px] transition-colors ${
                      fid === o.fid
                        ? "border-cyan-300 bg-cyan-300/15"
                        : "border-white/10 bg-white/5 hover:border-cyan-300/60"
                    }`}
                  >
                    {o.label}
                    {o.size_mb ? <span className="ml-1.5 text-slate-500">~{o.size_mb} MB</span> : null}
                  </button>
                ))}
              </div>
            ) : (
              <select
                value={abr}
                onChange={(e) => setAbr(e.target.value)}
                className="mt-3 w-full rounded-lg border border-white/10 bg-black/35 px-3 py-2 text-sm outline-none focus:border-emerald-300 sm:w-auto"
              >
                {info.audio_bitrates.map((b) => (
                  <option key={b} value={b}>
                    {b} kbps
                  </option>
                ))}
              </select>
            )}

            <button
              type="button"
              onClick={download}
              disabled={(mode === "video" && !fid) || dl !== null}
              className="mt-5 w-full rounded-xl bg-gradient-to-br from-emerald-300 to-cyan-300 py-3.5 font-bold text-[15px] text-teal-950 transition hover:brightness-110 disabled:opacity-45"
            >
              {dl ? "Downloading…" : mode === "audio"
                ? `Download MP3 · ${abr} kbps`
                : selectedOption
                  ? `Download MP4 · ${selectedOption.label}`
                  : "Select a quality"}
            </button>

            {dl && (
              <div className="mt-3 rounded-xl border border-cyan-300/30 bg-cyan-300/[0.06] p-3.5">
                <div className="flex items-center justify-between gap-2 text-xs">
                  <span className="min-w-0 truncate font-medium text-slate-200">{dl.label}</span>
                  <span className="shrink-0 tabular-nums text-slate-400">
                    {mb(dl.received)}
                    {dl.total != null && dl.total > 0 && (
                      <> / ~{mb(dl.total)} · {Math.min(99, Math.floor((dl.received / dl.total) * 100))}%</>
                    )}
                  </span>
                </div>
                <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-black/40">
                  {dl.total != null && dl.total > 0 ? (
                    <div
                      className="dl-bar h-full rounded-full bg-gradient-to-r from-cyan-300 to-emerald-300"
                      style={{ width: `${Math.min(100, (dl.received / dl.total) * 100)}%` }}
                    />
                  ) : (
                    <div className="dl-indeterminate h-full" />
                  )}
                </div>
                <div className="mt-2 flex items-center justify-between">
                  <span className="text-[11px] text-slate-500">
                    {streamToDisk ? "Saving straight to disk" : "Buffering in browser memory — keep this tab open"}
                  </span>
                  <button
                    type="button"
                    onClick={() => dlCtrl.current?.abort()}
                    className="rounded-lg border border-white/10 bg-white/5 px-2.5 py-1 text-[11px] text-slate-300 transition-colors hover:border-rose-400 hover:text-rose-300"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}

            {!dl && (
              <p className="mt-2 min-h-4 text-center text-xs text-slate-500">
                Streams straight to your browser — nothing is saved on the server.
              </p>
            )}

            {info?.playlist_id && !pl && (
              <button
                type="button"
                onClick={loadPlaylistHint}
                className="mt-3 w-full rounded-lg border border-cyan-300/30 bg-cyan-300/[0.07] px-3 py-2 text-xs font-medium text-cyan-200 transition-colors hover:border-cyan-300/60 hover:bg-cyan-300/[0.14]"
              >
                📃 This video is part of a playlist — load the whole list
              </button>
            )}
          </div>
        )}

        {pl && (
          <PlaylistPanel
            pl={pl}
            status={plStatus}
            activeId={info?.id ?? null}
            batchBusy={batchBusy}
            onPick={pickEntry}
            onDownloadAll={() => void downloadAll()}
            onCancelBatch={cancelBatch}
            onClose={() => {
              setPl(null);
              setPlStatus({});
            }}
          />
        )}

        <CookiesPanel />
      </section>

      <footer className="mt-7 text-center text-xs leading-relaxed text-slate-500">
        Rust API + Next.js UI · inspired by metube · powered by yt-dlp & ffmpeg.
        <br />
        Respect content owners and YouTube&apos;s Terms of Service.
        <br />
        <SelfHostModal />
      </footer>
    </main>
  );
}
