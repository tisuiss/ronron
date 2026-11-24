// The notifications of this run (core/notifications.ts), behind the header's
// bell: on the left, how many were new and what still waits; in the middle,
// every notification, newest first — a click goes where it belongs (the
// request, the reminder, the pill). Opening the list reads them: the ones that
// were new keep their dot while it stays on screen.

import { h, svg, clear, dot } from "./dom";
import { ICONS } from "./icons";
import { State } from "../core/state";
import { Notices, waitingViews, type Notice } from "../core/notifications";
import { spaceView, tile } from "./spaces";
import { REMINDERS_ID } from "../core/reminders";
import type { ViewActions, ViewHost } from "./views";
import { language, t } from "../i18n/i18n";

function when(at: number, now: number): string {
  const mins = Math.floor((now - at) / 60_000);
  if (mins < 1) return t("just now");
  if (mins < 60) return t("{n}m", { n: mins });
  return new Date(at).toLocaleTimeString(language(), { hour: "2-digit", minute: "2-digit" });
}

const KIND_ICON: Record<Notice["kind"], string> = {
  reminder: ICONS.calendar,
  alarm: ICONS.timer,
  request: ICONS.bang,
  finished: ICONS.check,
  error: ICONS.octagonX,
  message: ICONS.bubble,
  call: ICONS.bang,
  timer: ICONS.timer,
};

export function buildHistory(actions: ViewActions): ViewHost {
  const left = tile("sp-progress");
  const rows = h("div", { class: "sp-list" });
  const middle = tile("sp-tasks", h("div", { class: "sp-title" }, h("b", { text: t("Notifications") })), rows);
  const el = spaceView("history", left, middle, null);
  /** The ones unread when the list was opened: they keep their dot while it is shown. */
  let fresh = new Set<number>();
  let showing = false;
  let key = "";

  /** Where a notification leads: what still waits for it, else its pill. */
  function open(n: Notice) {
    actions.blip();
    const waiting = waitingViews(State);
    if (n.kind === "request" && waiting.some((v) => v === "approval" || v === "question")) {
      actions.setView(waiting.find((v) => v === "approval" || v === "question")!);
      return;
    }
    if (n.kind === "reminder" && State.reminder) return actions.setView("reminder");
    if (n.kind === "alarm" && State.alarm) return actions.setView("alarm");
    if (n.pillId && State.tasks.some((x) => x.id === n.pillId)) {
      actions.setFocus(n.pillId);
      actions.setView("overview");
      return;
    }
    if (n.pillId && State.headerIds.includes(n.pillId)) {
      const view = n.pillId === "integration_calendar" ? "agenda" : n.pillId === REMINDERS_ID ? "reminders" : null;
      if (view) actions.setView(view);
    }
  }

  function draw() {
    const now = Date.now();
    const list = Notices.items;
    const k = JSON.stringify([language(), Math.floor(now / 30_000), list.map((n) => [n.id, n.read]), [...fresh], waitingViews(State)]);
    if (k === key) return;
    key = k;

    clear(left);
    const waiting = waitingViews(State).length;
    left.append(
      h("span", { class: "sp-kicker", text: t("Notifications") }),
      h("span", { class: "rm-next", text: fresh.size > 0 ? t("{n} new", { n: fresh.size }) : t("Nothing new") }),
      waiting > 0 ? h("span", { class: "sp-dim sp-late", text: t("{n} waiting", { n: waiting }) }) : h("span", { class: "sp-dim", text: t("Since Ronron started") }),
    );
    if (list.length > 0) {
      left.append(h("button", { class: "sp-btn", style: "margin-top:6px;align-self:flex-start", text: t("Clear"), onclick: () => {
        actions.blip();
        fresh = new Set();
        Notices.clear();
      } }));
    }

    clear(rows);
    for (const n of list) {
      const row = h("button", { class: fresh.has(n.id) ? "sp-task hs-row new" : "sp-task hs-row" },
        h("span", { class: "hs-icon", style: `color:${n.color}` }, svg(KIND_ICON[n.kind], 12, { stroke: n.kind === "request" || n.kind === "call" ? 0 : 1.8 })),
        h("span", { class: "hs-text" },
          h("span", { class: "sp-name", text: n.title }),
          n.detail ? h("span", { class: "sp-dim hs-detail", text: n.detail }) : null),
        fresh.has(n.id) ? dot("#F4505E", 6) : null,
        h("span", { class: "sp-due", text: when(n.at, now) }));
      row.addEventListener("click", () => open(n));
      rows.append(row);
    }
    if (list.length === 0) rows.append(h("div", { class: "sp-empty", text: t("No notifications yet. Reminders, requests, finished turns, messages and calls show here.") }));
  }

  Notices.onChange(() => State.notify());

  return {
    el,
    sync() {
      if (State.view !== "history") {
        showing = false;
        return;
      }
      if (!showing) {
        // Just opened: what was unread is shown as new, and is read from now on.
        showing = true;
        fresh = new Set(Notices.items.filter((n) => !n.read).map((n) => n.id));
        key = "";
      }
      // News that comes while the list is open is new too.
      for (const n of Notices.items) if (!n.read) fresh.add(n.id);
      if (Notices.unread() > 0) queueMicrotask(() => Notices.markAllRead());
      draw();
    },
  };
}
