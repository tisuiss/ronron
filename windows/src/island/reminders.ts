// Calendar reminders: a few minutes before an event of a calendar set to remind
// (Settings → Integrations → Calendars), the island opens on it (views/reminder.ts)
// and stays until OK, Join or the event's start. One timer, set for the next one
// only, from the calendar data already in the island: nothing is polled.

import { State } from "../core/state";
import { Sound } from "../core/sound";
import { readBookings, readCalendars, type Booking, type CalendarInfo } from "../core/calendar";
import type { BotEmoteName, IslandViewName } from "../core/layout";
import { Notices } from "../core/notifications";
import { language } from "../i18n/i18n";

/** Snoozed reminders ring again this much later. */
export const SNOOZE_MIN = 5;
/** A reminder nobody answered goes this long after its event started (or at its end). */
const STAYS_AFTER_START_MS = 10 * 60_000;

const CALENDAR_ID = "integration_calendar";
const FIRED_KEY = "ronron.reminders.fired";
const FIRED_MAX = 300;
/** A reminder missed (computer asleep) still comes while the event has not started. */
const LATE_GRACE_MS = 0;

export interface ReminderHost {
  alert(view: IslandViewName): void;
  emote(name: BotEmoteName): void;
  setPinned(on: boolean): void;
  /** The reminder is over: the island folds (or goes back to what it showed). */
  closeReminder(): void;
}

export interface Due {
  booking: Booking;
  calendar: CalendarInfo;
  at: number;
  key: string;
}

/** Every reminder still to come or under way (before the event starts), earliest first. */
export function dueReminders(data: unknown, now: number, fired: ReadonlySet<string>): Due[] {
  const d = (data ?? {}) as Record<string, unknown>;
  const calendars = readCalendars(d.calendars);
  const bookings = readBookings(d.bookings) ?? [];
  const out: Due[] = [];
  for (const b of bookings) {
    if (b.allDay) continue;
    const calendar = calendars.find((c) => c.id === b.calendarId);
    if (!calendar || !calendar.shown || calendar.remind <= 0) continue;
    const start = b.start.getTime();
    if (start + LATE_GRACE_MS <= now) continue;
    const key = `${b.id}@${start}`;
    if (fired.has(key)) continue;
    out.push({ booking: b, calendar, at: start - calendar.remind * 60_000, key });
  }
  return out.sort((a, b) => a.at - b.at);
}

function loadFired(): string[] {
  try {
    const list = JSON.parse(window.localStorage.getItem(FIRED_KEY) ?? "[]");
    return Array.isArray(list) ? list.filter((x) => typeof x === "string") : [];
  } catch {
    return [];
  }
}

export class Reminders {
  private fired: string[] = loadFired();
  private timer: number | null = null;
  private key = "";
  private host: ReminderHost;

  constructor(host: ReminderHost) {
    this.host = host;
    State.subscribe(() => this.schedule());
    this.schedule();
  }

  private remember(key: string) {
    this.fired = [...this.fired.filter((k) => k !== key), key].slice(-FIRED_MAX);
    try {
      window.localStorage.setItem(FIRED_KEY, JSON.stringify(this.fired));
    } catch {
      // Kept for this run only.
    }
  }

  /** Sets the one timer, for the next reminder; nothing when the data has not changed. */
  private schedule() {
    const data = State.integrations[CALENDAR_ID]?.data;
    const next = dueReminders(data, Date.now(), new Set(this.fired))[0] ?? null;
    const key = next ? `${next.key}|${next.at}` : "";
    if (key === this.key) return;
    this.key = key;
    if (this.timer != null) window.clearTimeout(this.timer);
    this.timer = null;
    if (!next) return;
    // setTimeout holds 24 days at most; the data is read again long before.
    const wait = Math.min(Math.max(0, next.at - Date.now()), 2 ** 31 - 1);
    this.timer = window.setTimeout(() => this.fire(), wait);
  }

  private fire() {
    this.timer = null;
    this.key = "";
    const next = dueReminders(State.integrations[CALENDAR_ID]?.data, Date.now(), new Set(this.fired))[0];
    if (!next || next.at > Date.now() + 1000) {
      this.schedule();
      return;
    }
    // A permission card comes first: the reminder waits a little.
    if (State.pendingApproval) {
      this.timer = window.setTimeout(() => this.fire(), 20_000);
      return;
    }
    this.remember(next.key);
    this.ring(next.booking, next.calendar);
    this.schedule();
  }

  /** The island opens on the reminder and stays until OK, Join, a snooze, or a while after the start. */
  private ring(booking: Booking, calendar: CalendarInfo | null) {
    // A permission card comes first: the reminder waits a little.
    if (State.pendingApproval || State.reminder) {
      window.setTimeout(() => this.ring(booking, calendar), 20_000);
      return;
    }
    if (booking.end.getTime() <= Date.now()) return;
    State.reminder = { booking, calendar };
    const time = booking.start.toLocaleTimeString(language(), { hour: "2-digit", minute: "2-digit" });
    Notices.push({
      kind: "reminder", title: booking.title, detail: [time, calendar?.name ?? ""].filter(Boolean).join(" · "),
      color: calendar?.color, pillId: CALENDAR_ID, ref: `reminder:${booking.id}@${booking.start.getTime()}`,
    });
    this.host.setPinned(true);
    this.host.alert("reminder");
    this.host.emote("surprised");
    Sound.play("peek");
    // Gone a while after the event started (or at its end), if nobody answered: the history keeps it.
    const start = booking.start.getTime();
    const until = Math.min(booking.end.getTime(), start + STAYS_AFTER_START_MS) - Date.now();
    window.setTimeout(() => {
      if (State.reminder?.booking.id === booking.id) this.dismissed(false);
    }, Math.max(60_000, until));
  }

  /** OK, Join, or the event long under way: the island lets go. */
  dismissed(answered = true) {
    const r = State.reminder;
    if (r && answered) Notices.settle(`reminder:${r.booking.id}@${r.booking.start.getTime()}`);
    State.reminder = null;
    this.host.setPinned(false);
    this.host.closeReminder();
  }

  /** « In 5 min »: the island lets go and the reminder rings again then (while the event is not over). */
  snooze(minutes = SNOOZE_MIN) {
    const r = State.reminder;
    if (!r) return;
    this.dismissed();
    window.setTimeout(() => this.ring(r.booking, r.calendar), minutes * 60_000);
  }
}
