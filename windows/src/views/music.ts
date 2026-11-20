// The Music space (header tab "Music"): what plays on this computer — Spotify,
// YouTube Music, Amazon Music or anything else that shows in the system's media
// controls — with play/pause, previous and next. Two tiles: the cover with Ronron
// at its foot, then the players found, the track, its progress and the controls.
//
// Asked for every 1.5 s while the space is on screen, and never otherwise.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { State } from "../core/state";
import { MediaBridge, MUSIC_ID, clock, type MediaState } from "../core/media";
import type { ViewActions, ViewHost } from "./views";
import { language, t } from "../i18n/i18n";

const POLL_MS = 1500;

export function buildMusicSpace(actions: ViewActions): ViewHost {
  const cover = h("div", { class: "mu-cover" });
  const left = h("section", { class: "sp-tile sp-music-art" }, cover);
  const sources = h("div", { class: "sp-filters mu-sources" });
  const info = h("div", { class: "mu-info" });
  const bar = h("i");
  const progress = h("div", { class: "mu-bar" }, bar);
  const elapsed = h("span");
  const total = h("span");
  const times = h("div", { class: "mu-times" }, elapsed, total);
  const prev = h("button", { class: "mu-btn", title: t("Previous track"), "aria-label": t("Previous track") }, svg(ICONS.backward, 18));
  const play = h("button", { class: "mu-btn play" });
  const next = h("button", { class: "mu-btn", title: t("Next track"), "aria-label": t("Next track") }, svg(ICONS.forward, 18));
  const controls = h("div", { class: "mu-controls" }, prev, play, next);
  // The computer's volume: the speaker mutes, the slider sets the level.
  const muteBtn = h("button", { class: "mu-mute", title: t("Mute"), "aria-label": t("Mute") });
  const slider = h("input", { class: "mu-volume", type: "range", min: "0", max: "100", step: "1", "aria-label": t("Volume") }) as HTMLInputElement;
  const level = h("span", { class: "mu-level" });
  const volumeRow = h("div", { class: "mu-volume-row" }, muteBtn, slider, level);
  const empty = h("div", { class: "sp-empty mu-empty", text: t("Nothing is playing. Start Spotify, YouTube Music or Amazon Music on this computer.") });
  const main = h("section", { class: "sp-tile sp-music" }, sources, info, progress, times, controls, empty, volumeRow);
  const el = h("div", { class: "view" }, h("div", { class: "space music" }, left, main));

  let state: MediaState | null = null;
  /** The player picked from the chips; null = the one playing. */
  let wanted: string | null = null;
  let timer: number | null = null;
  let artKey = "";
  let busy = false;
  let key = "";

  async function refresh() {
    if (busy) return;
    busy = true;
    try {
      state = await MediaBridge.state(wanted);
    } finally {
      busy = false;
    }
    draw();
    const now = state?.now;
    if (now && now.artKey !== artKey) {
      artKey = now.artKey;
      const asked = now.artKey;
      const url = await MediaBridge.art(now.source);
      if (asked !== artKey) return;
      clear(cover);
      if (url && url.startsWith("data:image/")) cover.append(h("img", { src: url, alt: "" }));
      else cover.append(svg(ICONS.play, 26));
    } else if (!now && artKey) {
      artKey = "";
      clear(cover);
    }
  }

  async function control(action: "playPause" | "next" | "previous") {
    const now = state?.now;
    if (!now) return;
    actions.blip();
    // Shown at once; the player catches up.
    if (action === "playPause") {
      now.playing = !now.playing;
      key = "";
      draw();
    }
    await MediaBridge.control(now.source, action);
    window.setTimeout(() => void refresh(), 350);
  }

  /** The slider is being moved: what the computer says waits until it is let go. */
  let dragging = false;
  let sendTimer: number | null = null;
  slider.addEventListener("pointerdown", () => (dragging = true));
  slider.addEventListener("pointerup", () => (dragging = false));
  slider.addEventListener("input", () => {
    const v = Number(slider.value) / 100;
    level.textContent = `${slider.value}%`;
    if (state?.volume) state.volume = { level: v, muted: false };
    drawVolume();
    if (sendTimer != null) return;
    sendTimer = window.setTimeout(() => {
      sendTimer = null;
      void MediaBridge.setVolume(Number(slider.value) / 100);
    }, 80);
  });
  slider.addEventListener("change", () => {
    dragging = false;
    void MediaBridge.setVolume(Number(slider.value) / 100);
  });
  muteBtn.addEventListener("click", () => {
    const v = state?.volume;
    if (!v) return;
    actions.blip();
    v.muted = !v.muted;
    drawVolume();
    void MediaBridge.setMute(v.muted);
  });

  function drawVolume() {
    const v = state?.volume ?? null;
    volumeRow.style.display = v ? "" : "none";
    if (!v) return;
    const pct = Math.round(v.level * 100);
    if (!dragging && document.activeElement !== slider) slider.value = String(pct);
    level.textContent = v.muted ? t("Muted") : `${dragging ? slider.value : pct}%`;
    clear(muteBtn);
    muteBtn.append(svg(v.muted || pct === 0 ? ICONS.speakerSlashLine : ICONS.speakerLine, 16, { stroke: 1.6 }));
    muteBtn.classList.toggle("on", v.muted);
    muteBtn.setAttribute("title", v.muted ? t("Unmute") : t("Mute"));
    slider.style.setProperty("--fill", `${v.muted ? 0 : pct}%`);
  }

  prev.addEventListener("click", () => void control("previous"));
  play.addEventListener("click", () => void control("playPause"));
  next.addEventListener("click", () => void control("next"));

  function draw() {
    drawVolume();
    const now = state?.now ?? null;
    const list = state?.sources ?? [];
    const k = JSON.stringify([language(), wanted, list, now && { ...now, position: Math.floor(now.position) }]);
    if (k === key) return;
    key = k;

    clear(sources);
    if (list.length > 1) {
      for (const s of list) {
        sources.append(h("button", {
          class: s.id === now?.source ? "sp-chip on" : "sp-chip",
          title: s.id,
          onclick: () => {
            actions.blip();
            wanted = s.id;
            artKey = "";
            key = "";
            void refresh();
          },
        }, s.playing ? h("i", { class: "mu-live" }) : null, h("span", { text: s.name })));
      }
    }
    const shown = now !== null && (now.title !== "" || now.artist !== "");
    main.classList.toggle("idle", !shown);
    clear(info);
    if (!shown) return;
    const name = list.find((s) => s.id === now.source)?.name ?? "";
    info.append(
      h("span", { class: "sp-kicker", text: name }),
      h("b", { class: "mu-title", text: now.title || " " }),
      h("span", { class: "mu-artist", text: [now.artist, now.album].filter(Boolean).join(" · ") }),
    );
    const known = now.duration > 0;
    progress.style.visibility = known ? "" : "hidden";
    times.style.visibility = known ? "" : "hidden";
    bar.style.width = known ? `${Math.min(100, (now.position / now.duration) * 100).toFixed(1)}%` : "0";
    elapsed.textContent = clock(now.position);
    total.textContent = clock(now.duration);
    (prev as HTMLButtonElement).disabled = !now.canPrevious;
    (next as HTMLButtonElement).disabled = !now.canNext;
    clear(play);
    play.append(svg(now.playing ? ICONS.pause : ICONS.play, 20));
    play.setAttribute("title", now.playing ? t("Pause") : t("Play"));
    play.setAttribute("aria-label", now.playing ? t("Pause") : t("Play"));
  }

  function stop() {
    if (timer != null) window.clearInterval(timer);
    timer = null;
  }

  return {
    el,
    sync() {
      const on = State.view === "music" && State.mode === "expanded";
      if (!on) {
        stop();
        return;
      }
      if (!State.headerIds.includes(MUSIC_ID)) {
        window.setTimeout(() => {
          if (State.view === "music" && !State.headerIds.includes(MUSIC_ID)) actions.setView(State.defaultView());
        }, 0);
        return;
      }
      if (timer == null) {
        void refresh();
        timer = window.setInterval(() => {
          if (State.view === "music" && State.mode === "expanded") void refresh();
          else stop();
        }, POLL_MS);
      }
      draw();
    },
  };
}
