export interface VideoOption {
  fid: string;
  label: string;
  height: number;
  size_mb: number | null;
}

export interface InfoResponse {
  normalized_url: string;
  id?: string;
  title: string;
  uploader: string | null;
  duration_string: string;
  thumbnail: string | null;
  webpage_url?: string | null;
  video_options: VideoOption[];
  audio_bitrates: string[];
  playlist_id?: string | null;
}

export interface PlaylistEntry {
  id: string;
  index: number;
  title: string;
  url: string;
  uploader?: string | null;
  duration_string?: string | null;
  thumbnail?: string | null;
}

export interface PlaylistInfo {
  kind: "playlist" | "channel";
  title: string;
  url: string;
  total: number;
  total_claimed: number;
  truncated: boolean;
  entries: PlaylistEntry[];
}

export interface CookieStatus {
  active: boolean;
  name?: string;
  format?: string;
  cookies?: number;
  server_default?: boolean;
}

export interface Stats {
  online: number;
  total_downloads: number;
}
