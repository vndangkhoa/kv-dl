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
