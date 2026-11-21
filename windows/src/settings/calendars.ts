// Settings → Integrations → Calendars: the Microsoft 365 account's calendars and the
// iCal ones, each shown in Ronron or not, each with its reminder (src/island/reminders.ts).
// An iCal address opens its calendar to whoever has it: it goes to the credential
// store (`ical-url-<slot>`), never to settings.json.

import { h, clear, dot } from "../views/dom";
import { Bridge } from "../core/bridge";
import type { CalendarPref, IcalCalendar, Settings } from "../core/state";
import { t } from "../i18n/i18n";

export const REMIND_CHOICES = [0, 5, 10, 15, 30];
const ICAL_SLOTS = 8;
const ICAL_COLORS = ["#E879A9", "#34D399", "#A78BFA", "#F0913C", "#F5A524", "#6C9EFF", "#22D3EE", "#F4505E"];

export interface CalendarsAccess {
  settings(): Settings;
  save(): void;
}

export function remindLabel(minutes: number): string {
  return minutes > 0 ? t("{n} min before", { n: minutes }) : t("No reminder");
}

/** The choice saved for a calendar, or what applies until there is one. */
export function prefFor(prefs: CalendarPref[], id: string, shownByDefault: boolean): CalendarPref {
  return prefs.find((p) => p.id === id) ?? { id, shown: shownByDefault, remind: 0 };
}

export function setPref(settings: Settings, next: CalendarPref) {
  settings.calendarPrefs = [...(settings.calendarPrefs ?? []).filter((p) => p.id !== next.id), next];
}

/** The first iCal slot free, or null when all eight are taken. */
export function freeSlot(list: IcalCalendar[]): number | null {
  for (let i = 0; i < ICAL_SLOTS; i++) if (!list.some((c) => c.slot === i)) return i;
  return null;
}

export function calendarsSection(access: CalendarsAccess): HTMLElement {
  const list = h("div", { style: "display:flex;flex-direction:column;gap:8px" });
  const section = h("section", {},
    h("h2", {}, h("span", { text: t("Calendars") })),
    h("div", { class: "hint", text: t("The calendars Ronron shows, and the ones that remind you of their events: the island opens a few minutes before, with Join for an online meeting.") }),
    list,
    addForm());
  let microsoft: { id: string; name: string; color: string; default: boolean; kind?: "microsoft" | "google" }[] = [];

  function changed() {
    access.save();
    // Saved first, so the read that follows uses it.
    window.setTimeout(() => void Bridge.calendarRefresh(), 300);
  }

  function row(id: string, name: string, color: string, kind: string, shownByDefault: boolean, remove?: () => void): HTMLElement {
    const s = access.settings();
    const pref = prefFor(s.calendarPrefs ?? [], id, shownByDefault);
    const shown = h("button", { class: pref.shown ? "switch on" : "switch", title: t("Shown in Ronron"), "aria-label": t("Shown in Ronron") });
    shown.addEventListener("click", () => {
      const now = prefFor(access.settings().calendarPrefs ?? [], id, shownByDefault);
      setPref(access.settings(), { ...now, shown: !now.shown });
      shown.classList.toggle("on", !now.shown);
      remind.disabled = now.shown;
      changed();
    });
    const remind = h("select", { "aria-label": t("Reminder") }) as HTMLSelectElement;
    for (const m of REMIND_CHOICES) {
      const option = h("option", { value: String(m), text: remindLabel(m) }) as HTMLOptionElement;
      option.selected = m === pref.remind;
      remind.append(option);
    }
    remind.disabled = !pref.shown;
    remind.addEventListener("change", () => {
      const now = prefFor(access.settings().calendarPrefs ?? [], id, shownByDefault);
      setPref(access.settings(), { ...now, remind: Number(remind.value) });
      changed();
    });
    const r = h("div", { class: "row", style: "gap:10px" },
      shown,
      dot(color, 8),
      h("span", { style: "flex:1 1 auto;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap", text: name }),
      h("span", { class: "hint", style: "margin:0", text: kind }),
      remind);
    if (remove) r.append(h("button", { text: t("Remove"), onclick: remove }));
    return r;
  }

  function draw() {
    clear(list);
    for (const c of microsoft) list.append(row(c.id, c.name || t("Calendar"), c.color, c.kind === "google" ? "Google" : "Microsoft 365", c.default));
    for (const c of access.settings().icalCalendars ?? []) {
      list.append(row(`ical-${c.slot}`, c.name || `iCal ${c.slot + 1}`, c.color, "iCal", true, async () => {
        const s = access.settings();
        s.icalCalendars = (s.icalCalendars ?? []).filter((x) => x.slot !== c.slot);
        s.calendarPrefs = (s.calendarPrefs ?? []).filter((p) => p.id !== `ical-${c.slot}`);
        try {
          await Bridge.secretClear(`ical-url-${c.slot}`);
        } catch {
          // Already gone.
        }
        changed();
        draw();
      }));
    }
    if (list.childNodes.length === 0) {
      list.append(h("div", { class: "hint", text: t("Sign in to Microsoft 365 or Google above, or add an iCal address below.") }));
    }
  }

  function addForm(): HTMLElement {
    const name = h("input", { type: "text", placeholder: t("Name"), autocomplete: "off", spellcheck: "false", style: "width:120px" }) as HTMLInputElement;
    const url = h("input", { type: "password", placeholder: "webcal://… / https://….ics", autocomplete: "off", spellcheck: "false", style: "flex:1 1 auto;min-width:0" }) as HTMLInputElement;
    const add = h("button", { class: "primary", text: t("Add") }) as HTMLButtonElement;
    const note = h("div", { class: "hint" });
    add.addEventListener("click", async () => {
      const s = access.settings();
      const slot = freeSlot(s.icalCalendars ?? []);
      if (slot === null) {
        note.textContent = t("Eight iCal calendars at most.");
        return;
      }
      add.disabled = true;
      note.style.color = "";
      note.textContent = t("Checking…");
      try {
        const address = await Bridge.icalNormalize(url.value);
        const count = await Bridge.icalCheck(address);
        await Bridge.secretSet(`ical-url-${slot}`, address);
        s.icalCalendars = [...(s.icalCalendars ?? []), { slot, name: name.value.trim(), color: ICAL_COLORS[slot % ICAL_COLORS.length] }];
        name.value = "";
        url.value = "";
        note.textContent = t("Added: {n} events in the coming month.", { n: count });
        changed();
        draw();
      } catch (e) {
        note.style.color = "#f5a524";
        note.textContent = e instanceof Error ? e.message : String(e);
      } finally {
        add.disabled = false;
      }
    });
    return h("div", { style: "display:flex;flex-direction:column;gap:4px;margin-top:8px" },
      h("label", { text: t("Add an iCal calendar") }),
      h("div", { class: "row", style: "gap:8px" }, name, url, add),
      note,
      h("div", { class: "hint", text: t("The calendar's secret address (Google: Settings → the calendar → Secret address in iCal format; Outlook: Publish a calendar → ICS). It is kept in the credential store.") }));
  }

  draw();
  void Bridge.calendarSources().then((found) => {
    microsoft = found ?? [];
    draw();
  });
  return section;
}
