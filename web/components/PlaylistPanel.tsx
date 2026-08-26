"use client";

import type { PlaylistEntry, PlaylistInfo } from "@/lib/types";

export type RowStatus = "pending" | "downloading" | "done" | "failed";

export default function PlaylistPanel({
  pl,
  status,
  activeId,
  batchBusy,
  onPick,
  onDownloadAll,
  onCancelBatch,
  onClose,
}: {
  pl: PlaylistInfo;
  status: Record<string, RowStatus | undefined>;
  activeId: string | null;
  batchBusy: boolean;
  onPick: (e: PlaylistEntry) => void;
  onDownloadAll: () => void;
  onCancelBatch: () => void;
  onClose: () => void;
}) {
  const done = Object.values(status).filter((s) => s === "done").length;
  const failed = Object.values(status).filter((s) => s === "failed").length;
  const cap = Math.min(pl.entries.length, 100);

  return (
    <div className="mt-5">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="flex items-start gap-2 text-[15px] font-semibold leading-snug">
            <span className="shrink-0">{pl.kind === "channel" ? "📺" : "📃"}</span>
            <span className="line-clamp-2">{pl.title}</span>
          </h2>
          <p className="mt-1 text-xs text-slate-400">
            {pl.entries.length} videos
            {pl.truncated ? ` (of ${pl.total_claimed} — list capped)` : ""}
            {batchBusy ? ` · ${done} done${failed ? ` · ${failed} failed` : ""}` : ""}
          </p>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="shrink-0 rounded-lg border border-white/10 bg-white/5 px-2.5 py-1 text-[11px] text-slate-400 transition-colors hover:border-rose-400/60 hover:text-rose-300"
        >
          Close
        </button>
      </div>

      <div className="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          onClick={onDownloadAll}
          disabled={batchBusy}
          className="rounded-lg bg-gradient-to-br from-emerald-300 to-cyan-300 px-3.5 py-1.5 text-xs font-bold text-teal-950 transition hover:brightness-110 disabled:opacity-45"
        >
          ⬇ Download all{cap < pl.entries.length ? ` (first ${cap})` : ""}
        </button>
        {batchBusy && (
          <button
            type="button"
            onClick={onCancelBatch}
            className="rounded-lg border border-white/10 bg-white/5 px-3 py-1.5 text-xs text-slate-300 transition-colors hover:border-rose-400 hover:text-rose-300"
          >
            Stop
          </button>
        )}
        <span className="self-center text-[11px] text-slate-500">
          {batchBusy
            ? "Downloading one by one…"
            : "Saves at best quality (video+sound) · 1080p target"}
        </span>
      </div>

      <ol className="no-scrollbar mt-3 max-h-[420px] space-y-1 overflow-y-auto rounded-xl border border-white/10 bg-black/20 p-1.5">
        {pl.entries.map((e) => {
          const st = status[e.id];
          const active = activeId === e.id;
          return (
            <li key={e.id}>
              <div
                className={`flex items-center gap-2 rounded-lg p-1.5 transition-colors ${
                  active ? "bg-cyan-300/10" : "hover:bg-white/[0.04]"
                }`}
              >
                <button
                  type="button"
                  onClick={() => onPick(e)}
                  className="flex min-w-0 flex-1 items-center gap-2.5 text-left"
                  title="Load this video"
                >
                  <span className="w-5 shrink-0 text-center text-[11px] text-slate-500">
                    {e.index}
                  </span>
                  {e.thumbnail ? (
                    /* eslint-disable-next-line @next/next/no-img-element */
                    <img
                      src={e.thumbnail}
                      alt=""
                      referrerPolicy="no-referrer"
                      className="aspect-video w-16 shrink-0 rounded border border-white/10 bg-black object-cover"
                    />
                  ) : (
                    <span className="aspect-video w-16 shrink-0 rounded border border-white/10 bg-black/40" />
                  )}
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13px] text-slate-200">{e.title}</span>
                    <span className="block truncate text-[11px] text-slate-500">
                      {[e.uploader, e.duration_string].filter(Boolean).join(" · ")}
                    </span>
                  </span>
                </button>
                <span className="w-10 shrink-0 text-center text-[11px]" aria-live="polite">
                  {st === "downloading" && <span className="text-cyan-300">↓…</span>}
                  {st === "done" && <span className="text-emerald-300">✓</span>}
                  {st === "failed" && <span className="text-rose-300">✗</span>}
                </span>
              </div>
            </li>
          );
        })}
      </ol>
      <p className="mt-2 text-[11px] text-slate-500">
        Click a video to load it above — preview, pick a quality, download.
      </p>
    </div>
  );
}
