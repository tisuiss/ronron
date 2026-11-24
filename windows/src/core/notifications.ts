// The notifications of this run, newest first: what the island showed or
// badged — a calendar reminder, one of your reminders ringing, a request from
// an agent, a turn finished or failed, a message, a missed call, a Clockify
// suggestion — so one covered by another is not lost. The header's history
// button counts the unread ones and opens the list (views/history.ts).
// In memory only: nothing is written to disk, the list starts empty at launch.

import type { IslandViewName } from "./layout";

export type NoticeKind = "reminder" | "alarm" | "request" | "finished" | "error" | "message" | "call" | "timer";

export interface Notice {
  id: number;
  at: number;
  kind: NoticeKind;
  title: string;
  detail: string;
  color: string;
  /** The pill it belongs to: a click focuses it. */
  pillId: string | null;
  /** What it stands for (a request id, a reminder's key): marks it read once dealt with. */
  ref: string | null;
  read: boolean;
}

export type NewNotice = Omit<Notice, "id" | "at" | "read" | "ref" | "pillId" | "color"> &
  Partial<Pick<Notice, "at" | "ref" | "pillId" | "color">>;

export const MAX_NOTICES = 60;
/** The same news twice within this time is one notice. */
const SAME_WITHIN_MS = 60_000;

export class NoticeLog {
  items: Notice[] = [];
  private next = 1;
  private listeners: (() => void)[] = [];

  onChange(fn: () => void) {
    this.listeners.push(fn);
  }

  private changed() {
    for (const fn of this.listeners) fn();
  }

  push(n: NewNotice): Notice | null {
    const at = n.at ?? Date.now();
    const same = this.items.find((x) =>
      x.kind === n.kind && x.title === n.title && x.detail === n.detail && Math.abs(at - x.at) < SAME_WITHIN_MS);
    if (same) return null;
    const notice: Notice = {
      id: this.next++, at, kind: n.kind, title: n.title, detail: n.detail,
      color: n.color ?? "#8a8f98", pillId: n.pillId ?? null, ref: n.ref ?? null, read: false,
    };
    this.items = [notice, ...this.items].slice(0, MAX_NOTICES);
    this.changed();
    return notice;
  }

  unread(): number {
    return this.items.filter((n) => !n.read).length;
  }

  /** Dealt with elsewhere (the request answered, the reminder's OK): no longer unread. */
  settle(ref: string) {
    let any = false;
    for (const n of this.items) {
      if (n.ref === ref && !n.read) {
        n.read = true;
        any = true;
      }
    }
    if (any) this.changed();
  }

  markAllRead() {
    if (this.items.every((n) => n.read)) return;
    for (const n of this.items) n.read = true;
    this.changed();
  }

  clear() {
    if (this.items.length === 0) return;
    this.items = [];
    this.changed();
  }
}

export const Notices = new NoticeLog();

/**
 * What waits on the island, most urgent first: a request, then a calendar
 * reminder, then one of your reminders ringing. The island goes back to the
 * next one once the one on screen is answered (State.defaultView).
 */
export function waitingViews(state: {
  pendingApproval: { questions?: unknown } | null;
  reminder: unknown;
  alarm: unknown;
}): IslandViewName[] {
  const out: IslandViewName[] = [];
  if (state.pendingApproval) out.push(state.pendingApproval.questions ? "question" : "approval");
  if (state.reminder) out.push("reminder");
  if (state.alarm) out.push("alarm");
  return out;
}

/** Views a lesser alert (a turn finished, a pill's news) never covers. */
export const WAITING_VIEWS: ReadonlySet<IslandViewName> = new Set(["approval", "question", "reminder", "alarm"]);
