// A calendar reminder (island/reminders.ts): Ronron on the left, how long until the
// event, its title, time and calendar, then Join (an online meeting), Open, OK.

import { h, clear, dot } from "./dom";
import { State } from "../core/state";
import { Bridge } from "../core/bridge";
import { untilLabel } from "./spaces";
import type { ViewActions, ViewHost } from "./views";
import { language, t } from "../i18n/i18n";

export function buildReminder(actions: ViewActions): ViewHost {
  const kicker = h("span", { class: "sp-kicker" });
  const title = h("b", { class: "rm-title" });
  const line = h("div", { class: "rm-line" });
  const buttons = h("div", { class: "rm-buttons" });
  const body = h("div", { class: "rm-body" }, kicker, title, line, buttons);
  const el = h("div", { class: "view" }, h("div", { class: "card rm-card" }, body));
  let key = "";

  return {
    el,
    sync() {
      const r = State.reminder;
      if (!r) return;
      const now = Date.now();
      const b = r.booking;
      const k = JSON.stringify([language(), b.id, b.start.getTime(), Math.floor(now / 30_000)]);
      if (k === key) return;
      key = k;
      const left = b.start.getTime() - now;
      kicker.textContent = left > 30_000 ? untilLabel(left) : t("Now");
      title.textContent = b.title || t("Meeting");
      const time = (d: Date) => d.toLocaleTimeString(language(), { hour: "2-digit", minute: "2-digit" });
      clear(line);
      if (r.calendar) line.append(dot(r.calendar.color, 7));
      line.append(h("span", { text: [`${time(b.start)} – ${time(b.end)}`, r.calendar?.name ?? "", b.attendeeNotes].filter(Boolean).join(" · ") }));
      clear(buttons);
      if (b.joinUrl) {
        const url = b.joinUrl;
        buttons.append(h("button", { class: "sp-btn primary rm-join", text: t("Join the meeting"), onclick: () => {
          void Bridge.openUrl(url);
          actions.dismissReminder();
        } }));
      }
      if (b.webUrl) {
        const url = b.webUrl;
        buttons.append(h("button", { class: "sp-btn", text: t("Open"), onclick: () => void Bridge.openUrl(url) }));
      }
      buttons.append(h("button", { class: "sp-btn rm-ok", text: t("OK"), onclick: () => actions.dismissReminder() }));
    },
  };
}
