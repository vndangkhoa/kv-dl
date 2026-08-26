"use client";

import { useCallback, useEffect, useState } from "react";
import type { InfoResponse, Stats } from "@/lib/types";
import Odometer from "@/components/Odometer";
import DemoTypewriter from "@/components/DemoTypewriter";
import CookiesPanel from "@/components/CookiesPanel";
import SelfHostModal from "@/components/SelfHostModal";

type Mode = "video" | "audio";

export default function Home() {
  const [url, setUrl] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [info, setInfo] = useState<InfoResponse | null>(null);
  const [mode, setMode] = useState<Mode>("video");
  const [fid, setFid] = useState<string | null>(null);
  const [abr, setAbr] = useState("192");
  const [stats, setStats] = useState<Stats | null>(null);

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

  async function fetchInfo(e?: React.FormEvent) {
    e?.preventDefault();
    if (!url.trim()) return;
    setLoading(true);
    setError("");
    setInfo(null);
    try {
      const res = await fetch("/api/info", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ url }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? `HTTP ${res.status}`);
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
    }
  }

  function download() {
    if (!info) return;
    const params = new URLSearchParams({ url, mode });
    if (mode === "video") {
      if (!fid) return;
      params.set("fid", fid);
    } else {
      params.set("abr", abr);
    }
    const a = document.createElement("a");
    a.href = "/api/download?" + params.toString();
    document.body.appendChild(a);
    a.click();
    a.remove();
  }

  const selectedOption = info?.video_options.find((o) => o.fid === fid);

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
        <form onSubmit={fetchInfo} className="flex gap-2.5">
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
            className="rounded-xl bg-gradient-to-br from-cyan-300 to-emerald-300 px-6 py-3 font-bold text-[15px] text-teal-950 transition hover:brightness-110 disabled:opacity-55"
          >
            {loading ? "…" : "Fetch"}
          </button>
        </form>
        {error && <p className="mt-3 whitespace-pre-wrap break-words text-sm text-rose-300">{error}</p>}

        {info && (
          <div className="mt-5">
            <div className="flex items-center gap-3.5">
              {info.thumbnail && (
                <div className="relative shrink-0">
                  {/* eslint-disable-next-line @next/next/no-img-element */}
                  <img
                    src={info.thumbnail}
                    alt=""
                    referrerPolicy="no-referrer"
                    className="aspect-video w-40 rounded-lg border border-white/10 bg-black object-cover"
                  />
                  {info.duration_string && (
                    <span className="absolute bottom-1.5 right-1.5 rounded bg-black/75 px-1.5 py-0.5 text-[11px]">
                      {info.duration_string}
                    </span>
                  )}
                </div>
              )}
              <div className="min-w-0">
                <h2 className="line-clamp-3 text-[15px] leading-snug">{info.title}</h2>
                {info.uploader && <p className="mt-1 text-xs text-slate-400">{info.uploader}</p>}
              </div>
            </div>

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
                className="mt-3 rounded-lg border border-white/10 bg-black/35 px-3 py-2 text-sm outline-none focus:border-emerald-300"
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
              disabled={mode === "video" && !fid}
              className="mt-5 w-full rounded-xl bg-gradient-to-br from-emerald-300 to-cyan-300 py-3.5 font-bold text-[15px] text-teal-950 transition hover:brightness-110 disabled:opacity-45"
            >
              {mode === "audio"
                ? `Download MP3 · ${abr} kbps`
                : selectedOption
                  ? `Download MP4 · ${selectedOption.label}`
                  : "Select a quality"}
            </button>
            <p className="mt-2 min-h-4 text-center text-xs text-slate-500">
              Streams straight to your browser — nothing is saved on the server.
            </p>
          </div>
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
