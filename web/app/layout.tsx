import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "KV-DL — YouTube Downloader",
  description:
    "Paste any YouTube link — download video+audio merged, or MP3. Streams straight to you; nothing is stored.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className="min-h-screen antialiased">{children}</body>
    </html>
  );
}
