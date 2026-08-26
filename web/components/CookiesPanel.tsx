"use client";

import { useEffect, useState } from "react";
import type { CookieStatus } from "@/lib/types";

export default function CookiesPanel() {
  const [status, setStatus] = useState<CookieStatus | null>(null);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [busy, setBusy] = useState(false);

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

  async function onUpload(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const fd = new FormData();
    fd.append("file", file);
    setBusy(true);
    setMsg(null);
    try {
      const res = await fetch("/api/cookies/upload", { method: "POST", body: fd });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? `HTTP ${res.status}`);
      setStatus(data);
      setMsg({ text: `Cookies stored in memory for this session (${data.cookies} cookies).`, ok: true });
    } catch (err) {
      setMsg({ text: "Upload failed: " + (err as Error).message, ok: false });
    } finally {
      setBusy(false);
      e.target.value = "";
    }
  }

  async function onClear() {
    setBusy(true);
    try {
      await fetch("/api/cookies/clear", { method: "POST" });
      setStatus({ active: false });
      setMsg({ text: "Cookies removed from the server's memory.", ok: true });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="mt-4 rounded-xl border border-dashed border-white/10 p-3.5">
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
              <b className="text-slate-200">Cookies active</b> — {status.name}
              {status.cookies ? ` · ${status.cookies} cookies` : ""}
            </>
          ) : (
            "No cookies (age-restricted videos may fail)"
          )}
        </span>
        <span className="flex-1" />
        <label
          className={`cursor-pointer rounded-lg border border-white/10 bg-white/5 px-3 py-1.5 text-xs transition-colors hover:border-cyan-300 ${
            busy ? "pointer-events-none opacity-50" : ""
          }`}
        >
          Upload cookies.txt
          <input type="file" accept=".txt,text/plain" className="hidden" onChange={onUpload} disabled={busy} />
        </label>
        {status?.active && (
          <button
            type="button"
            onClick={onClear}
            disabled={busy}
            className="rounded-lg border border-white/10 bg-white/5 px-3 py-1.5 text-xs text-slate-300 transition-colors hover:border-rose-400 hover:text-rose-300"
          >
            Remove
          </button>
        )}
      </div>

      <details className="mt-2.5 text-[13px]">
        <summary className="cursor-pointer select-none text-[12.5px] text-cyan-300">
          How do I get my cookies.txt? (for age-restricted / bot-checked videos)
        </summary>
        <ol className="my-2.5 list-decimal space-y-0.5 pl-5 text-[13px] leading-relaxed text-slate-400">
          <li>Log in to <b className="text-slate-300">youtube.com</b> in your browser.</li>
          <li>
            Install an exporter extension — Chrome/Edge/Brave:{" "}
            <i>&ldquo;Get cookies.txt LOCALLY&rdquo;</i>, Firefox: <i>&ldquo;cookies.txt&rdquo;</i>.
          </li>
          <li>On youtube.com click the extension → Export (Netscape format).</li>
          <li>Upload the .txt here with the button above.</li>
        </ol>
        <p className="rounded-r-lg border-l-[3px] border-emerald-400 bg-emerald-400/10 px-3 py-2 text-[11.5px] leading-relaxed text-slate-400">
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
