// A reminder ringing (island/alarms.ts): Ronron on the left, the reminder's text and
// time, then In 10 min, In 1 h (it rings again then) and Done (it goes).

import { h, clear } from "./dom";
import { State } from "../core/state";
import { whenLabel } from "../core/reminders";
import type { ViewActions, ViewHost } from "./views";
import { language, t } from "../i18n/i18n";

export function buildAlarm(actions: ViewActions): ViewHost {
  const kicker = h("span", { class: "sp-kicker", text: t("Reminder") });
  const title = h("b", { class: "rm-title" });
  const line = h("div", { class: "rm-line" });
  const buttons = h("div", { class: "rm-buttons" });
  const body = h("div", { class: "rm-body" }, kicker, title, line, buttons);
  const el = h("div", { class: "view" }, h("div", { class: "card rm-card" }, body));
  let key = "";

  return {
    el,
    sync() {
      const r = State.alarm;
      if (!r) return;
      const k = JSON.stringify([language(), r.id, r.at, r.text]);
      if (k === key) return;
      key = k;
      kicker.textContent = t("Reminder");
      title.textContent = r.text;
      line.textContent = whenLabel(r.at, new Date());
      clear(buttons);
      buttons.append(
        h("button", { class: "sp-btn", text: t("In {n} min", { n: 10 }), onclick: () => actions.snoozeAlarm(10) }),
        h("button", { class: "sp-btn", text: t("In 1 h"), onclick: () => actions.snoozeAlarm(60) }),
        h("button", { class: "sp-btn primary rm-ok", text: t("Done"), onclick: () => actions.alarmDone() }),
      );
    },
  };
}
