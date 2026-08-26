import type { NextConfig } from "next";

const API = process.env.API_URL ?? "http://127.0.0.1:8080";

// STATIC_EXPORT=1 -> fully static bundle (web/out) served by the Rust binary.
// Otherwise -> dev/prod server with rewrites proxying /api/* to the Rust API.
const nextConfig: NextConfig =
  process.env.STATIC_EXPORT === "1"
    ? { output: "export", images: { unoptimized: true } }
    : {
        async rewrites() {
          return [{ source: "/api/:path*", destination: `${API}/api/:path*` }];
        },
      };

export default nextConfig;
