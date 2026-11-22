// Reminders ringing: when a reminder's time comes (core/reminders.ts), the island
// opens on it (views/alarm.ts) and stays until Done or a snooze. One timer, set for
// the soonest reminder; one that came due while the computer slept rings at once.
// Rust keeps the list: Done removes the reminder, a snooze moves it on, and the
// `reminders` event that follows sets the next timer.

import { State } from "../core/state";
import { Sound } from "../core/sound";
import { ReminderList, RemindersBridge, type Reminder } from "../core/reminders";
import type { BotEmoteName, IslandViewName } from "../core/layout";

export interface AlarmHost {
  alert(view: IslandViewName): void;
  emote(name: BotEmoteName): void;
  setPinned(on: boolean): void;
  /** The reminder is answered: the island folds (or goes back to what it showed). */
  closeAlarm(): void;
}

/**
 * The reminder to ring now, if any: the soonest whose time has come, not on screen
 * already, and not one just answered (`answered`: id → the time it was answered for,
 * until Rust's list says it is gone or moved).
 */
export function dueAlarm(list: readonly Reminder[], now: number, showing: string | null, answered: ReadonlyMap<string, number> = new Map()): Reminder | null {
  return list.find((r) => r.at <= now && r.id !== showing && answered.get(r.id) !== r.at) ?? null;
}

export class Alarms {
  private timer: number | null = null;
  private key = "";
  private host: AlarmHost;
  /** Answered (Done or snoozed) but maybe still in the list until Rust's event comes back. */
  private answered = new Map<string, number>();

  constructor(host: AlarmHost) {
    this.host = host;
    State.subscribe(() => this.schedule());
    this.schedule();
  }

  /** Sets the one timer, for the soonest reminder; nothing when the list has not changed. */
  private schedule() {
    // One at a time: the next waits until this one is answered.
    if (State.alarm) return;
    for (const [id, at] of this.answered) {
      if (!ReminderList.items.some((r) => r.id === id && r.at === at)) this.answered.delete(id);
    }
    const next = ReminderList.items.find((r) => this.answered.get(r.id) !== r.at) ?? null;
    const key = next ? `${next.id}|${next.at}` : "";
    if (key === this.key) return;
    this.key = key;
    if (this.timer != null) window.clearTimeout(this.timer);
    this.timer = null;
    if (!next) return;
    // setTimeout holds 24 days at most: a later reminder is looked at again then.
    const wait = Math.min(Math.max(0, next.at - Date.now()), 2 ** 31 - 1);
    this.timer = window.setTimeout(() => this.fire(), wait);
  }

  private fire() {
    this.timer = null;
    this.key = "";
    const due = dueAlarm(ReminderList.items, Date.now() + 500, State.alarm?.id ?? null, this.answered);
    if (!due) {
      this.schedule();
      return;
    }
    // A permission card comes first: the reminder waits a little.
    if (State.pendingApproval) {
      this.timer = window.setTimeout(() => this.fire(), 20_000);
      return;
    }
    State.alarm = due;
    this.host.setPinned(true);
    this.host.alert("alarm");
    this.host.emote("surprised");
    Sound.play("peek");
  }

  private close() {
    const r = State.alarm;
    if (r) this.answered.set(r.id, r.at);
    State.alarm = null;
    this.host.setPinned(false);
    this.host.closeAlarm();
    this.schedule();
  }

  /** Done: the reminder goes. */
  done() {
    const r = State.alarm;
    if (!r) return;
    this.close();
    void RemindersBridge.remove(r.id).catch(() => {});
  }

  /** Rings again in `minutes`. */
  snooze(minutes: number) {
    const r = State.alarm;
    if (!r) return;
    this.close();
    void RemindersBridge.snooze(r.id, minutes).catch(() => {});
  }
}
