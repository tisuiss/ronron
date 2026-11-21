// The Music tab (src-tauri/src/media.rs): what plays on this computer, read and
// driven through the system's media controls — no account.

import { invoke } from "@tauri-apps/api/core";
import { IS_TAURI } from "./bridge";

/** The header entry of the Music tab (settings.headerPills). Not a pill. */
export const MUSIC_ID = "music";

export interface MediaSource {
  id: string;
  name: string;
  playing: boolean;
}

export interface NowPlaying {
  source: string;
  title: string;
  artist: string;
  album: string;
  playing: boolean;
  position: number;
  duration: number;
  artKey: string;
  canPrevious: boolean;
  canNext: boolean;
}

export interface Volume {
  /** 0 to 1. */
  level: number;
  muted: boolean;
}

export interface MediaState {
  sources: MediaSource[];
  now: NowPlaying | null;
  /** The playing app's own volume (its stream in the system mixer, never the
   * computer's), when it can be read: null while it has no sound yet. */
  volume?: Volume | null;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!IS_TAURI) return null;
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    console.error(`[ronron] ${cmd} failed`, err);
    return null;
  }
}

export const MediaBridge = {
  state: (source: string | null) => call<MediaState>("media_state", { source }),
  control: (source: string, action: "playPause" | "next" | "previous") => call<boolean>("media_control", { source, action }),
  art: (source: string) => call<string | null>("media_art", { source }),
  /** The volume of the app playing as `source` only — never the computer's. */
  setVolume: (source: string, level: number) => call<boolean>("media_set_volume", { source, level }),
  setMute: (source: string, muted: boolean) => call<boolean>("media_set_mute", { source, muted }),
};

/** "3:07", "1:02:45". */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}
