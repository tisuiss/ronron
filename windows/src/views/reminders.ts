// The Reminders tab: reminders kept on this computer (core/reminders.ts). On the
// left, the next one, Ronron at its foot; in the middle, every reminder (✕ removes
// it) and the field that adds one — a quick time (in 10 min, in 1 h, this evening,
// tomorrow morning) or a date and time of your own.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { State } from "../core/state";
import {
  REMINDERS_ID, ReminderList, RemindersBridge, fromLocalInput, quickTimes, toLocalInput, whenLabel,
} from "../core/reminders";
import { bindField, leftHeader, releaseOnLeave, spaceView, tile, untilLabel } from "./spaces";
import type { ViewActions, ViewHost } from "./views";
import { language, t } from "../i18n/i18n";

export function buildRemindersSpace(actions: ViewActions): ViewHost {
  const left = tile("sp-progress");
  const rows = h("div", { class: "sp-list" });
  const input = h("input", { class: "sp-add-input", type: "text", maxlength: "300", spellcheck: "true", placeholder: t("Remind me to…") }) as HTMLInputElement;
  const custom = h("input", { class: "rm-when", type: "datetime-local", "aria-label": t("Pick a date and time") }) as HTMLInputElement;
  const chips = h("div", { class: "sp-filters rm-chips" });
  const addBtn = h("button", { class: "sp-add-go", title: t("Add"), "aria-label": t("Add") }, svg(ICONS.plus, 12, { stroke: 2.2 }));
  const error = h("div", { class: "sp-error" });
  const middle = tile("sp-tasks", h("div", { class: "sp-title" }, h("b", { text: t("Reminders") })), rows, error,
    h("div", { class: "rm-add" }, chips, h("label", { class: "sp-add" }, input, custom, addBtn)));
  const el = spaceView("reminders", left, middle, null);
  /** The time picked from the chips (ms); null = the field's own date and time. */
  let chosen: number | null = null;
  let key = "";
  let busy = false;

  bindField(input, actions);
  bindField(custom, actions);
  releaseOnLeave("reminders", input, actions);

  async function submit() {
    const text = input.value.trim();
    if (!text || busy) {
      if (!text) input.focus();
      return;
    }
    const at = chosen ?? fromLocalInput(custom.value);
    if (at === null) {
      error.textContent = t("Pick when: a time below, or a date and time.");
      return;
    }
    busy = true;
    try {
      await RemindersBridge.add(text, at);
      input.value = "";
      chosen = null;
      custom.value = "";
      error.textContent = "";
      actions.holdOpen(false);
      key = "";
      draw();
    } catch (e) {
      error.textContent = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  input.addEventListener("keydown", (e) => {
    const k = (e as KeyboardEvent).key;
    if (k === "Enter") {
      e.preventDefault();
      void submit();
    } else if (k === "Escape" && input.value) {
      e.preventDefault();
      input.value = "";
      actions.holdOpen(false);
    } else if (k === "Escape") {
      return;
    }
    e.stopPropagation();
  });
  custom.addEventListener("input", () => {
    chosen = null;
    key = "";
    draw();
  });
  addBtn.addEventListener("click", () => void submit());

  function draw() {
    const now = new Date();
    const list = ReminderList.items;
    const quick = quickTimes(now);
    const k = JSON.stringify([language(), Math.floor(now.getTime() / 30_000), list, chosen, custom.value]);
    if (k === key) return;
    key = k;

    // Left: the next reminder, or nothing planned.
    clear(left);
    const next = list[0];
    if (next) {
      const ms = next.at - now.getTime();
      left.append(
        h("span", { class: ms <= 0 ? "sp-kicker sp-late" : "sp-kicker", text: ms <= 0 ? t("Now") : untilLabel(ms) }),
        h("span", { class: "rm-next", text: next.text }),
        h("span", { class: "sp-dim", text: whenLabel(next.at, now) }),
      );
    } else {
      left.append(h("span", { class: "sp-kicker", text: t("Reminders") }), h("span", { class: "sp-dim", text: t("Nothing planned") }));
    }

    // Middle: every reminder, soonest first.
    clear(rows);
    for (const r of list) {
      const late = r.at <= now.getTime();
      rows.append(h("div", { class: "sp-task", "data-reminder": r.id },
        h("span", { class: "rm-bell", "aria-hidden": "true" }, svg(ICONS.timer, 12, { stroke: 1.8 })),
        h("span", { class: "sp-name", text: r.text }),
        h("span", { class: late ? "sp-due late" : "sp-due", text: whenLabel(r.at, now) }),
        h("button", {
          class: "sp-icon rm-remove", title: t("Remove"), "aria-label": t("Remove"),
          onclick: () => {
            actions.blip();
            void RemindersBridge.remove(r.id).catch((e) => (error.textContent = String(e)));
          },
        }, svg(ICONS.xmark, 10, { stroke: 2 })),
      ));
    }
    if (list.length === 0) rows.append(h("div", { class: "sp-empty", text: t("No reminders yet. Write one below and pick when.") }));

    // The quick times, the one picked lit.
    clear(chips);
    for (const q of quick) {
      const on = chosen !== null && Math.abs(chosen - q.at) < 60_000;
      chips.append(h("button", {
        class: on ? "sp-chip on" : "sp-chip",
        text: q.label,
        onclick: () => {
          actions.blip();
          chosen = on ? null : q.at;
          custom.value = chosen === null ? "" : toLocalInput(chosen);
          key = "";
          draw();
        },
      }));
    }
    custom.min = toLocalInput(now.getTime());
  }

  return {
    el,
    sync() {
      if (State.view !== "reminders") return;
      if (leftHeader("reminders", REMINDERS_ID, actions)) return;
      draw();
    },
    focus() {
      input.focus();
    },
  };
}
