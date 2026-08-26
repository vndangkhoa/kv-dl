"use client";

import { useEffect, useState } from "react";
import type { CookieStatus } from "@/lib/types";

const FORMAT_LABELS: Record<string, string> = {
  netscape: "Netscape cookies.txt",
  json: "JSON",
  header: "Cookie header",
  "set-cookie": "Set-Cookie",
};

export default function CookiesPanel() {
  const [status, setStatus] = useState<CookieStatus | null>(null);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const [mode, setMode] = useState<"paste" | "file">("paste");
  const [pasted, setPasted] = useState("");
  const [open, setOpen] = useState(false);

  async function refresh() {
    try {
      const res = await fetch("/api/cookies/status");
      setStatus(await res.json());
    } catch {
      setStatus({ active: false });
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function send(body: BodyInit, isJson = false) {
    setBusy(true);
    setMsg(null);
    try {
      const res = await fetch("/api/cookies/upload", {
        method: "POST",
        body,
        ...(isJson ? { headers: { "Content-Type": "application/json" } } : {}),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? `HTTP ${res.status}`);
      setStatus(data);
      const fmt = FORMAT_LABELS[data.format as string] ?? data.format;
      setMsg({
        text: `Saved in memory (${fmt ?? "unknown format"} · ${data.cookies} cookies).`,
        ok: true,
      });
      setOpen(false);
      setPasted("");
    } catch (err) {
      setMsg({ text: (isJson ? "Paste failed: " : "Upload failed: ") + (err as Error).message, ok: false });
    } finally {
      setBusy(false);
    }
  }

  function onFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const fd = new FormData();
    fd.append("file", file);
    void send(fd).finally(() => {
      e.target.value = "";
    });
  }

  function onPasteSubmit(e?: React.FormEvent) {
    e?.preventDefault();
    if (!pasted.trim()) return;
    void send(JSON.stringify({ text: pasted }), true);
  }

  return (
    <div className="mt-4 rounded-xl border border-dashed border-white/10 p-3.5">
      {/* status row */}
      <div className="flex flex-wrap items-center gap-2.5">
        <span
          className={`h-2.5 w-2.5 shrink-0 rounded-full ${
            status?.active ? "bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.7)]" : "bg-slate-500"
          }`}
        />
        <span className="text-[13px] text-slate-400">
          {status === null ? (
            "Checking cookies…"
          ) : status.active ? (
            <>
              <b className="text-slate-200">Cookies active</b>
              {status.cookies ? ` · ${status.cookies} cookies` : ""}
              {status.format && FORMAT_LABELS[status.format]
                ? ` · ${FORMAT_LABELS[status.format]}`
                : ""}
            </>
          ) : (
            "No cookies (age-restricted videos may fail)"
          )}
        </span>
        <span className="flex-1" />
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          disabled={busy}
          className={`rounded-lg border px-3 py-1.5 text-xs transition-colors ${
            open
              ? "border-white/15 bg-white/10 text-slate-200"
              : "border-cyan-300/40 bg-cyan-300/10 text-cyan-200 hover:border-cyan-300"
          }`}
        >
          {open ? "Cancel" : status?.active ? "Replace…" : "Add cookies…"}
        </button>
        {status?.active && (
          <button
            type="button"
            onClick={async () => {
              setBusy(true);
              try {
                await fetch("/api/cookies/clear", { method: "POST" });
                setStatus({ active: false });
                setMsg({ text: "Cookies removed from the server's memory.", ok: true });
              } finally {
                setBusy(false);
              }
            }}
            disabled={busy}
            className="rounded-lg border border-white/10 bg-white/5 px-3 py-1.5 text-xs text-slate-300 transition-colors hover:border-rose-400 hover:text-rose-300"
          >
            Remove
          </button>
        )}
      </div>

      {/* input area */}
      {open && (
        <div className="mt-3">
          <div className="mb-2 inline-flex rounded-lg border border-white/10 bg-black/30 p-0.5">
            {(["paste", "file"] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => setMode(m)}
                className={`rounded-md px-3 py-1 text-xs font-medium transition-colors ${
                  mode === m ? "bg-cyan-300 text-teal-950" : "text-slate-400 hover:text-slate-200"
                }`}
              >
                {m === "paste" ? "Paste" : "Upload file"}
              </button>
            ))}
          </div>

          {mode === "paste" ? (
            <form onSubmit={onPasteSubmit}>
              <textarea
                value={pasted}
                onChange={(e) => setPasted(e.target.value)}
                placeholder={
                  "Easiest: on youtube.com press F12 → Network → click any request → copy the whole “cookie:” header and paste it here.\n\nAlso fine: Netscape cookies.txt content, JSON export, or Set-Cookie lines."
                }
                rows={6}
                spellCheck={false}
                autoComplete="off"
                className="w-full resize-y rounded-xl border border-white/10 bg-black/35 p-3 font-mono text-[11.5px] leading-relaxed outline-none transition-colors placeholder:text-slate-600 focus:border-cyan-300"
              />
              <div className="mt-2 flex items-center gap-2">
                <button
                  type="submit"
                  disabled={busy || !pasted.trim()}
                  className="rounded-lg bg-gradient-to-br from-cyan-300 to-emerald-300 px-4 py-1.5 text-xs font-bold text-teal-950 transition hover:brightness-110 disabled:opacity-45"
                >
                  {busy ? "Saving…" : "Use these cookies"}
                </button>
                <span className="text-[11px] text-slate-500">
                  Kept in RAM only, bound to your session.
                </span>
              </div>
            </form>
          ) : (
            <label
              className={`flex cursor-pointer flex-col items-center justify-center gap-1 rounded-xl border border-dashed border-white/15 bg-black/20 px-3 py-5 text-center transition-colors hover:border-cyan-300 ${
                busy ? "pointer-events-none opacity-50" : ""
              }`}
            >
              <span className="text-[13px] text-slate-300">
                Choose a cookies file — <b>Netscape .txt</b>, <b>JSON</b>, or any export
              </span>
              <span className="text-[11px] text-slate-500">max 512 KB · never stored on disk</span>
              <input type="file" accept=".txt,.json,text/plain,application/json" className="hidden" onChange={onFile} disabled={busy} />
            </label>
          )}
        </div>
      )}

      {/* help */}
      <details className="mt-2.5 text-[13px]" open={!status?.active}>
        <summary className="cursor-pointer select-none text-[12.5px] text-cyan-300">
          How do I add cookies? (for age-restricted / bot-checked videos)
        </summary>
        <ol className="my-2.5 list-decimal space-y-0.5 pl-5 text-[13px] leading-relaxed text-slate-400">
          <li>
            Log in to <b className="text-slate-300">youtube.com</b> in your browser.
          </li>
          <li>
            Press <b>F12</b> → <b>Network</b> tab → refresh → click any request → under{" "}
            <i>Request Headers</i> copy the whole <b>cookie:</b> value.
          </li>
          <li>Paste it above and hit “Use these cookies”. Done.</li>
        </ol>
        <p className="text-[12px] leading-relaxed text-slate-500">
          Prefer a file? Any of these work: Netscape <code>cookies.txt</code> (e.g. from the{" "}
          <i>&ldquo;Get cookies.txt LOCALLY&rdquo;</i> extension), JSON exports (yt-dlp style),
          <code> Cookie:</code> header strings, or <code>Set-Cookie</code> lines. Formats are
          detected automatically.
        </p>
        <p className="mt-2 rounded-r-lg border-l-[3px] border-emerald-400 bg-emerald-400/10 px-3 py-2 text-[11.5px] leading-relaxed text-slate-400">
          Your cookies are protected at the highest level practical for a webapp: they live only in
          this server&apos;s memory, bound to your private session — never written to disk, never
          logged, never sent back to any browser. Erased when you click Remove, your session expires,
          or the server restarts.
        </p>
      </details>

      {msg && (
        <p className={`m-0 mt-2 min-h-4 text-xs ${msg.ok ? "text-emerald-300" : "text-rose-300"}`}>{msg.text}</p>
      )}
    </div>
  );
}
