"use client";

import { useEffect, useRef, useState } from "react";

const BASE = "youtube";
const TLD_COM = ".com";
const PATH = "/watch?v=2GJfWMYCWY0";

// Shown when we can't derive a domain (localhost, IPs, …).
const FALLBACK_TLD = ".vndns.net";

// The mirror form is "<base>.<hosting-domain>"; derive the suffix from
// wherever this instance is served so every self-hosted copy demos itself.
function mirrorTld(): string {
  const host = window.location.hostname.toLowerCase();
  const labels = host.replace(/^www\./, "").split(".").filter(Boolean);
  const isIp = /^\d{1,3}(\.\d{1,3}){3}$/.test(host) || host.includes(":");
  const isDomain = !isIp && host !== "localhost" && labels.length >= 2;
  return isDomain ? `.${labels.slice(-2).join(".")}` : FALLBACK_TLD;
}

export default function DemoTypewriter() {
  const [base, setBase] = useState("");
  const [tld, setTld] = useState("");
  const [path, setPath] = useState("");
  const [swapped, setSwapped] = useState(false);
  const [done, setDone] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const TLD_SWAP = mirrorTld();
    const sleep = (ms: number) =>
      new Promise<void>((r) => {
        timer.current = setTimeout(r, ms);
      });

    async function type(set: (s: string) => void, text: string, speed: number) {
      for (let i = 1; i <= text.length; i++) {
        set(text.slice(0, i));
        await sleep(speed);
      }
    }

    async function loop() {
      for (;;) {
        setBase(""); setTld(""); setPath(""); setSwapped(false); setDone(false);
        await type(setBase, BASE, 55);
        await sleep(150);
        await type(setTld, TLD_COM, 55);
        await sleep(200);
        await type(setPath, PATH, 22);
        await sleep(1300);

        // swap .com -> .<hosting-domain>
        for (let i = TLD_COM.length; i >= 0; i--) {
          setTld(TLD_COM.slice(0, i));
          await sleep(35);
        }
        setSwapped(true);
        await type(setTld, TLD_SWAP, 55);
        await sleep(250);
        setDone(true);
        await sleep(2800);
      }
    }

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setBase(BASE); setTld(TLD_SWAP); setSwapped(true); setPath(PATH); setDone(true);
      return;
    }
    void loop();
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, []);

  return (
    <div className="mt-6 flex flex-col items-center gap-2" aria-hidden>
      <div className="text-[11px] uppercase tracking-widest text-slate-400">Just swap the domain</div>
      <div
        className={`inline-flex max-w-full items-baseline overflow-hidden whitespace-nowrap rounded-full border px-4 py-2.5 font-mono text-sm transition-colors ${
          done ? "border-emerald-400/60 shadow-[0_0_24px_rgba(52,211,153,0.18)]" : "border-white/10"
        } bg-black/30`}
      >
        <span className="text-slate-500">https://</span>
        <span>{base}</span>
        <span className={swapped ? "font-bold text-cyan-300 [text-shadow:0_0_14px_rgba(34,211,238,0.55)]" : ""}>{tld}</span>
        <span className="text-slate-500">{path}</span>
        <span className="caret translate-y-[3px]" />
      </div>
      <div
        className={`flex items-center gap-1 text-xs text-emerald-300 transition-all duration-300 ${
          done ? "opacity-100 translate-y-0" : "opacity-0 translate-y-1"
        }`}
      >
        <span className="arrow-bounce inline-block">↓</span>
        Ready to download — video+sound or audio only
      </div>
    </div>
  );
}
