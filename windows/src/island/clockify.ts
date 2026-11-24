// Clockify's suggestions in the island (core/clockify.ts says when): an agent
// at work with no timer running, a meeting starting, a timer left running.
// A new one lights the Clockify pill and plays a notification sound; the card
// shows it with its button. Nothing starts or stops without a click.

import { State } from "../core/state";
import { Sound } from "../core/sound";
import { CLOCKIFY_ID, folderName, readClockify, suggestion, type Suggestion } from "../core/clockify";
import { Notices } from "../core/notifications";
import { t } from "../i18n/i18n";

const dismissed = new Set<string>();
let current: Suggestion | null = null;

/** The suggestion shown now, if any. */
export function clockifySuggestion(): Suggestion | null {
  return current;
}

/** The running timer, for the closed island: what (or its project), its colour, since when. */
export function clockifyTimer(): { label: string; color: string; start: number } | null {
  if (!State.tasks.some((t) => t.id === CLOCKIFY_ID)) return null;
  const r = readClockify(State.integrations[CLOCKIFY_ID]?.data)?.running;
  if (!r) return null;
  return { label: r.description || r.projectName, color: r.projectColor, start: r.start };
}

/** "Not now": that suggestion is not offered again (today, for a folder). */
export function dismissClockify(key: string) {
  dismissed.add(key);
  Notices.settle(`clockify:${key}`);
  checkClockify();
}

function bookings(id: string): { id: string; title: string; start: unknown; end: unknown }[] {
  const data = State.integrations[id]?.data as Record<string, unknown> | undefined;
  const list = Array.isArray(data?.bookings) ? (data.bookings as Record<string, unknown>[]) : [];
  return list.map((b) => ({ id: `${id}:${String(b.id ?? b.start)}`, title: String(b.title ?? ""), start: b.start, end: b.end }));
}

/** Works out the suggestion again (on each redraw of the island and every 10 s). Cheap. */
export function checkClockify(now = Date.now()) {
  const task = State.tasks.find((t) => t.id === CLOCKIFY_ID);
  const info = State.integrations[CLOCKIFY_ID];
  const data = task && info?.loaded && !info.error ? readClockify(info.data) : null;
  const next = suggestion({
    now,
    data,
    agents: State.tasks
      .filter((t) => t.sessionCwd)
      .map((t) => ({ name: t.name, folder: t.sessionCwd ?? null, busy: !["idle", "sleeping", "finished"].includes(t.state) })),
    events: [...bookings("integration_calendar"), ...bookings("integration_calcom")],
    folders: State.settings.clockifyFolders ?? {},
    dismissed,
  });
  if ((next?.key ?? null) === (current?.key ?? null)) {
    current = next;
    return;
  }
  current = next;
  if (task) {
    if (next) {
      if (State.focusId !== CLOCKIFY_ID) task.pillBadge = "approval";
      Notices.push({
        kind: "timer", title: "Clockify", color: task.color, pillId: CLOCKIFY_ID, ref: `clockify:${next.key}`,
        detail: next.kind === "agent" ? t("{agent} is working in {folder}. Start a timer?", { agent: next.agent, folder: folderName(next.folder) })
          : next.kind === "meeting" ? t("« {title} » is starting. Start a timer?", { title: next.title })
          : t("The timer has been running for {n} h. Still on it?", { n: Math.floor((now - next.entry.start) / 3600_000) }),
      });
      Sound.play("peek", "notifications");
    } else if (task.pillBadge === "approval") {
      task.pillBadge = null;
    }
  }
  // Called while the island draws: the redraw it asks for comes after.
  queueMicrotask(() => State.notify());
}
