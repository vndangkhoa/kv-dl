"use client";

import { useRef, useState } from "react";

function CodeBlock({
  code,
  label,
  caption,
}: {
  code: string;
  label: string;
  caption?: string;
}) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      /* clipboard unavailable */
    }
  }

  return (
    <div className="mt-3">
      <div className="overflow-hidden rounded-xl border border-white/10 bg-black/50">
        <div className="flex items-center justify-between border-b border-white/[0.07] bg-white/[0.04] py-1.5 pl-3.5 pr-2">
          <span className="font-mono text-[11px] uppercase tracking-wider text-slate-500">
            {label}
          </span>
          <button
            type="button"
            onClick={copy}
            className={`rounded-lg border px-2.5 py-1 text-[11px] font-semibold transition-colors ${
              copied
                ? "border-emerald-300/60 bg-emerald-300/15 text-emerald-200"
                : "border-white/15 bg-white/5 text-slate-300 hover:border-cyan-300/60 hover:text-cyan-200"
            }`}
          >
            {copied ? "Copied ✓" : "Copy"}
          </button>
        </div>
        <pre className="overflow-x-auto p-3.5 text-left font-mono text-[12px] leading-relaxed text-sky-100">
          {code}
        </pre>
      </div>
      {caption && <p className="mt-1.5 text-[12px] leading-relaxed text-slate-500">{caption}</p>}
    </div>
  );
}

function WorksWhen({ children }: { children: React.ReactNode }) {
  return (
    <p className="mt-2.5 rounded-lg border border-emerald-300/20 bg-emerald-300/[0.06] px-3 py-2 text-[12px] leading-relaxed text-emerald-200/90">
      <b className="font-semibold">✓ It worked when…</b> {children}
    </p>
  );
}

function Step({
  n,
  title,
  time,
  last = false,
  children,
}: {
  n: number;
  title: string;
  time: string;
  last?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[30px_1fr] gap-x-3.5">
      <div className="flex flex-col items-center">
        <span className="z-10 flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-full bg-gradient-to-br from-cyan-300 to-emerald-300 text-[13px] font-bold text-teal-950 shadow-md">
          {n}
        </span>
        {!last && <span className="mt-1.5 w-px flex-1 bg-white/10" />}
      </div>
      <div className={last ? "" : "pb-7"}>
        <div className="flex flex-wrap items-center gap-2 pt-1">
          <h4 className="text-[15px] font-semibold text-slate-100">{title}</h4>
          <span className="rounded-full border border-white/10 bg-white/5 px-2 py-0.5 text-[10.5px] font-medium text-slate-400">
            ⏱ {time}
          </span>
        </div>
        {children}
      </div>
    </div>
  );
}

const TROUBLES: { problem: string; fix: React.ReactNode }[] = [
  {
    problem: "Page won't load at all",
    fix: (
      <>
        On the server run <code className="text-cyan-200">docker compose ps</code> and{" "}
        <code className="text-cyan-200">docker compose logs --tail 50</code>.
      </>
    ),
  },
  {
    problem: "The domain doesn't connect",
    fix: <>DNS can take a few minutes. Double-check the A record points at your server&apos;s IP.</>,
  },
  {
    problem: "Browser warns about the certificate",
    fix: (
      <>
        The first certificate can take ~a minute. Check Caddy with{" "}
        <code className="text-cyan-200">journalctl -u caddy -f</code>.
      </>
    ),
  },
  {
    problem: "Big downloads stall halfway",
    fix: (
      <>
        nginx users: keep <code className="text-cyan-200">proxy_buffering off;</code> on{" "}
        <code className="text-cyan-200">/api/</code> (the Caddy config needs nothing extra).
      </>
    ),
  },
  {
    problem: "Port 8080 is already in use",
    fix: (
      <>
        In <code className="text-cyan-200">docker-compose.yml</code> change{" "}
        <code className="text-cyan-200">&quot;8080:8080&quot;</code> to e.g.{" "}
        <code className="text-cyan-200">&quot;9090:8080&quot;</code>, then point your proxy at{" "}
        <code className="text-cyan-200">127.0.0.1:9090</code>.
      </>
    ),
  },
];

export default function SelfHostModal() {
  const dlg = useRef<HTMLDialogElement>(null);

  return (
    <>
      <button
        type="button"
        onClick={() => dlg.current?.showModal()}
        className="mt-1 cursor-pointer text-[12.5px] text-cyan-300 underline decoration-cyan-300/40 underline-offset-[3px] hover:decoration-cyan-300"
      >
        Run your own copy — self-host guide
      </button>

      <dialog
        ref={dlg}
        onClick={(e) => e.target === dlg.current && dlg.current.close()}
        className="max-h-[88vh] w-[min(720px,calc(100vw-32px))] max-w-none overflow-y-auto rounded-2xl border border-white/10 bg-[#10151f] p-6 text-left text-slate-200 shadow-2xl backdrop:bg-black/70"
      >
        {/* ── header ─────────────────────────────────────────── */}
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-start gap-3">
            <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-cyan-300/25 to-emerald-300/25 text-xl">
              🚀
            </span>
            <div>
              <h3 className="m-0 text-[17px] font-semibold">Put KV-DL on your own domain</h3>
              <p className="mt-1 text-[13px] leading-relaxed text-slate-400">
                No coding needed — paste each command in order. End result: your own private
                downloader at an address like{" "}
                <code className="text-cyan-200">https://dl.example.net</code>.
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={() => dlg.current?.close()}
            className="rounded-lg px-2 text-2xl leading-none text-slate-400 hover:bg-white/5 hover:text-white"
            aria-label="Close"
          >
            ×
          </button>
        </div>

        <div className="mt-3 flex flex-wrap gap-1.5">
          {["⏱ ~15 min total", "📋 copy & paste only", "🧩 4 short steps"].map((t) => (
            <span
              key={t}
              className="rounded-full border border-white/10 bg-white/[0.04] px-2.5 py-1 text-[11px] text-slate-400"
            >
              {t}
            </span>
          ))}
        </div>

        {/* ── prerequisites ──────────────────────────────────── */}
        <div className="mt-4 rounded-xl border border-cyan-300/20 bg-cyan-300/[0.06] p-4">
          <h4 className="text-[12px] font-semibold uppercase tracking-wide text-cyan-300">
            First, check you have these 3 things
          </h4>
          <ul className="mt-2 space-y-1.5 text-[13px] leading-relaxed text-slate-300">
            <li>
              <span className="mr-1.5">🖥️</span>
              <b>A cheap Linux server (&ldquo;VPS&quot;)</b> — Hetzner, DigitalOcean, Vultr… Ubuntu
              22.04 is perfect. From ~$5/month.
            </li>
            <li>
              <span className="mr-1.5">🌐</span>
              <b>A domain name</b> — bought anywhere. We&apos;ll use{" "}
              <code className="text-cyan-200">dl.example.net</code> as the example.
            </li>
            <li>
              <span className="mr-1.5">🐳</span>
              <b>Docker on that server</b> — test with{" "}
              <code className="text-cyan-200">docker --version</code>. Missing? Install with one
              line:{" "}
              <code className="text-[11.5px] text-cyan-200">
                curl -fsSL https://get.docker.com | sh
              </code>
            </li>
          </ul>
        </div>

        {/* ── placeholder legend ─────────────────────────────── */}
        <div className="mt-3 rounded-xl border border-amber-300/20 bg-amber-300/[0.05] p-4">
          <h4 className="text-[12px] font-semibold uppercase tracking-wide text-amber-300/90">
            Two example values below — swap in yours
          </h4>
          <ul className="mt-2 space-y-1 text-[12.5px] leading-relaxed text-slate-400">
            <li>
              <code className="text-amber-200">dl.example.net</code> → your own subdomain, e.g.{" "}
              <code className="text-amber-200">dl.yourname.com</code>
            </li>
            <li>
              <code className="text-amber-200">203.0.113.10</code> → your server&apos;s public IP
              (run <code className="text-amber-200">curl ifconfig.me</code> on the server to see it)
            </li>
          </ul>
        </div>

        {/* ── steps (timeline) ───────────────────────────────── */}
        <div className="mt-6">
          <Step n={1} title="Start the app on your server" time="~5 min, mostly build time">
            <p className="mt-1.5 text-[13px] leading-relaxed text-slate-400">
              Connect to your server, download this project, and let Docker build and start it. One
              container serves both the web UI and the API on port <b>8080</b>.
            </p>
            <CodeBlock
              label="terminal"
              code={
                "ssh root@203.0.113.10\ngit clone <this-repo> kv-dl && cd kv-dl\ndocker compose up --build -d"
              }
              caption="Line 1 logs you in from your own computer. The first build takes a few minutes — -d keeps it running in the background, even after you log out."
            />
            <WorksWhen>
              opening <code className="text-emerald-200">http://203.0.113.10:8080</code> in your
              browser shows the app.
            </WorksWhen>
          </Step>

          <Step n={2} title="Point your domain at the server" time="~3 min + short wait">
            <p className="mt-1.5 text-[13px] leading-relaxed text-slate-400">
              Tell the internet where your subdomain lives. In the DNS settings of the site where
              you bought the domain, add one <b>&ldquo;A record&rdquo;</b> — a single row:
            </p>
            <CodeBlock
              label="DNS record"
              code={"Type   Name              Value          TTL\nA      dl.example.net.   203.0.113.10   300"}
              caption="Copy the row into your domain's DNS panel, swapping in your subdomain and IP. New records usually work within minutes."
            />
            <WorksWhen>
              <code className="text-emerald-200">ping dl.example.net</code> replies with your
              server&apos;s IP.
            </WorksWhen>
          </Step>

          <Step n={3} title="Switch on HTTPS (the padlock)" time="~5 min">
            <p className="mt-1.5 text-[13px] leading-relaxed text-slate-400">
              <b>Caddy</b> is a tiny web server that gets you a valid HTTPS certificate
              automatically — free, zero manual setup. Install it, paste 3 lines, reload:
            </p>
            <CodeBlock
              label="terminal — install & apply"
              code={
                "apt install -y caddy\nnano /etc/caddy/Caddyfile   # paste the block below, save\nsystemctl reload caddy"
              }
              caption="Debian/Ubuntu shown — add sudo if you're not root. Other systems: caddyserver.com/docs/install."
            />
            <CodeBlock
              label="/etc/caddy/Caddyfile"
              code={"dl.example.net {\n    reverse_proxy 127.0.0.1:8080}"}
              caption="That's the whole config — Caddy fetches and renews the certificate for you."
            />
            <WorksWhen>
              <code className="text-emerald-200">https://dl.example.net</code> opens with a padlock
              (first load can take ~1 min while the certificate is issued).
            </WorksWhen>

            <details className="group mt-3">
              <summary className="cursor-pointer select-none text-[12.5px] font-medium text-slate-400 hover:text-cyan-300">
                Prefer nginx instead? (a bit more manual)
              </summary>
              <div className="mt-1">
                <CodeBlock
                  label="nginx server block"
                  code={
                    "server {\n    listen 443 ssl;\n    server_name dl.example.net;\n    # ... your certificate lines (e.g. certbot) ...\n\n    location /api/ {\n        proxy_pass      http://127.0.0.1:8080;\n        proxy_buffering off;   # important — keeps downloads streaming\n    }\n    location / {\n        proxy_pass http://127.0.0.1:8080;\n    }\n}"
                  }
                  caption="proxy_buffering off on /api/ matters — without it, long downloads can stall."
                />
              </div>
            </details>
          </Step>

          <Step n={4} title="Add a secret key (recommended)" time="2 min" last>
            <p className="mt-1.5 text-[13px] leading-relaxed text-slate-400">
              Open <code className="text-cyan-200">docker-compose.yml</code> in the project folder
              and fill in the <code className="text-cyan-200">environment:</code> section:
            </p>
            <CodeBlock
              label="docker-compose.yml"
              code={
                "environment:\n  SECRET_KEY: paste-any-long-random-string\n  SECURE_COOKIES: \"1\"\n  # optional — remember the download counter:\n  # STATS_FILE: /data/stats.json"
              }
              caption="Then apply with: docker compose up -d. Using STATS_FILE? Uncomment the two volumes: lines in the same file too."
            />
            <WorksWhen>
              you stay logged in after a <code className="text-emerald-200">docker compose restart</code>.
            </WorksWhen>
          </Step>
        </div>

        {/* ── done ───────────────────────────────────────────── */}
        <div className="mt-2 rounded-xl border border-emerald-300/25 bg-emerald-300/[0.07] p-4">
          <p className="text-[13.5px] font-semibold text-emerald-200">
            🎉 Done — open <code className="text-emerald-200">https://dl.example.net</code>
          </p>
          <p className="mt-1.5 text-[12.5px] leading-relaxed text-slate-400">
            Paste a normal <code className="text-cyan-200">youtube.com</code> link and hit Fetch.
            Your copy accepts regular YouTube links — any domain-swap trick you&apos;ve seen
            elsewhere is just that instance&apos;s convention, not a requirement. And links shaped
            like <code className="text-cyan-200">https://youtube.&lt;your-domain&gt;/watch?v=…</code>{" "}
            work here too, automatically.
          </p>
        </div>

        {/* ── troubleshooting ────────────────────────────────── */}
        <details className="mt-4">
          <summary className="cursor-pointer select-none text-[12.5px] font-medium text-slate-400 hover:text-cyan-300">
            Something not working? Quick checks
          </summary>
          <ul className="mt-2.5 space-y-2 text-[12.5px] leading-relaxed text-slate-400">
            {TROUBLES.map((t) => (
              <li key={t.problem} className="grid grid-cols-[150px_1fr] gap-2">
                <span className="font-medium text-slate-300">{t.problem}</span>
                <span>→ {t.fix}</span>
              </li>
            ))}
          </ul>
        </details>
      </dialog>
    </>
  );
}
