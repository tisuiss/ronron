// Reminders kept on this computer (src-tauri/src/reminders.rs): the list the island
// shows in the Reminders tab and rings for (island/alarms.ts). Rust keeps the file;
// every change comes back as a `reminders` event with the whole list.

import { invoke } from "@tauri-apps/api/core";
import { IS_TAURI, onEvent } from "./bridge";
import { State } from "./state";
import { language, t } from "../i18n/i18n";

/** The Reminders tab in the header (Settings → Accounts & services → Integrations). Not a pill. */
export const REMINDERS_ID = "reminders";

export interface Reminder {
  id: string;
  text: string;
  /** When it rings, ms since 1970. */
  at: number;
}

export function readReminders(value: unknown): Reminder[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter((r) => r && typeof r.id === "string" && typeof r.text === "string" && typeof r.at === "number" && Number.isFinite(r.at))
    .map((r) => ({ id: r.id, text: r.text, at: r.at }))
    .sort((a, b) => a.at - b.at);
}

/** The list as last read. */
export const ReminderList = { items: [] as Reminder[] };

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!IS_TAURI) throw new Error("not in Tauri");
  return invoke<T>(cmd, args);
}

function apply(list: unknown) {
  ReminderList.items = readReminders(list);
  State.notify();
}

export const RemindersBridge = {
  list: () => call<unknown>("reminders_list"),
  add: (text: string, at: number) => call<unknown>("reminders_add", { text, at: Math.round(at) }),
  remove: (id: string) => call<void>("reminders_remove", { id }),
  snooze: (id: string, minutes: number) => call<void>("reminders_snooze", { id, minutes }),
};

let started = false;

/** Reads the list once and follows its changes. */
export function startReminders() {
  if (started) return;
  started = true;
  void onEvent<unknown>("reminders", apply);
  RemindersBridge.list().then(apply, () => {});
}

const time = (d: Date) => d.toLocaleTimeString(language(), { hour: "2-digit", minute: "2-digit" });

function atHour(day: Date, hour: number): Date {
  const d = new Date(day);
  d.setHours(hour, 0, 0, 0);
  return d;
}

/** The quick choices under the field: in 10 min, in 1 h, this evening, tomorrow morning. */
export function quickTimes(now: Date): { label: string; at: number }[] {
  const out = [
    { label: t("In {n} min", { n: 10 }), at: now.getTime() + 10 * 60_000 },
    { label: t("In 1 h"), at: now.getTime() + 60 * 60_000 },
  ];
  const evening = atHour(now, 18);
  // This evening only while it is more than half an hour away.
  if (evening.getTime() - now.getTime() > 30 * 60_000) out.push({ label: `${t("This evening")} ${time(evening)}`, at: evening.getTime() });
  const tomorrow = new Date(now);
  tomorrow.setDate(now.getDate() + 1);
  const morning = atHour(tomorrow, 9);
  out.push({ label: `${t("Tomorrow")} ${time(morning)}`, at: morning.getTime() });
  return out;
}

function sameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/** "Today 15:00", "Tomorrow 09:00", "Mon 24 Nov 18:00". */
export function whenLabel(at: number, now: Date): string {
  const d = new Date(at);
  const tomorrow = new Date(now);
  tomorrow.setDate(now.getDate() + 1);
  if (sameDay(d, now)) return `${t("Today")} ${time(d)}`;
  if (sameDay(d, tomorrow)) return `${t("Tomorrow")} ${time(d)}`;
  return `${d.toLocaleDateString(language(), { weekday: "short", day: "numeric", month: "short" })} ${time(d)}`;
}

/** A `datetime-local` field's value ("2026-11-22T15:00") → ms, in this computer's time. */
export function fromLocalInput(value: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(value.trim());
  if (!m) return null;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), Number(m[4]), Number(m[5]));
  return Number.isNaN(d.getTime()) ? null : d.getTime();
}

/** ms → a `datetime-local` value, in this computer's time. */
export function toLocalInput(at: number): string {
  const d = new Date(at);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}
