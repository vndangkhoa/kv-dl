export type PlaylistIntent = { kind: "playlist" | "channel" };

/** Pure playlist/channel links trigger the list flow automatically. */
export function detectPlaylist(url: string): PlaylistIntent | null {
  let u: URL;
  try {
    const t = url.trim();
    u = new URL(t.includes("://") ? t : `https://${t}`);
  } catch {
    return null;
  }
  const host = u.hostname.toLowerCase();
  if (!host.includes("youtube")) return null;
  const p = u.pathname;
  if (p.startsWith("/playlist")) return { kind: "playlist" };
  if (
    p.startsWith("/@") ||
    p.startsWith("/channel/") ||
    p.startsWith("/c/") ||
    p.startsWith("/user/")
  ) {
    return { kind: "channel" };
  }
  return null;
}
