// The Microsoft 365 spaces: Calendar, Mail and To Do, each with a view of its own
// opened from the header tabs. Three tiles per space, no pills: the pills stay in
// the overview. Data comes from the header pills (src-tauri/src/calendar365.rs,
// mail/graph.rs, todo/), the actions are the cards' own (calendar, mail, todo).
//
// Each view is built once and redrawn from a key, so a search field or a task
// being typed keeps its text and the keyboard while the data refreshes.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { State } from "../core/state";
import { Bridge } from "../core/bridge";
import { dayKey, readBookings, readCalendars, type Booking } from "../core/calendar";
import {
  MAIL_FILTERS, MAIL_ID, MailBridge, filterTitle, messageId, senderName,
  type MailCardData, type MailFilter, type MailMessage,
} from "../core/mail";
import type { IslandViewName } from "../core/layout";
import { MUSIC_ID } from "../core/media";
import { timeAgo } from "./integrations";
import { TODO_ID, dueLabel, overdueCount, todayKey, doneTodayCount, todoActions, type TodoTask } from "./todo";
import type { ViewActions, ViewHost } from "./views";
import { language, t } from "../i18n/i18n";

export const CALENDAR_ID = "integration_calendar";
const ACCENT = "#F0913C";
/** Colour bars for the day's events, in turn. */
const EVENT_COLORS = [ACCENT, "#6C9EFF", "#E879A9", "#34D399", "#A78BFA"];

/** Header pill → its space. */
export const SPACE_OF: Record<string, IslandViewName> = {
  [CALENDAR_ID]: "agenda",
  [MAIL_ID]: "inbox",
  [TODO_ID]: "tasks",
  [MUSIC_ID]: "music",
};

export function isSpace(view: IslandViewName): boolean {
  return view === "agenda" || view === "inbox" || view === "tasks" || view === "music";
}

function tile(cls: string, ...children: (Node | null)[]): HTMLElement {
  return h("section", { class: `sp-tile ${cls}` }, ...children);
}

function spaceView(cls: string, left: HTMLElement, middle: HTMLElement, right: HTMLElement | null): HTMLElement {
  return h("div", { class: "view" }, h("div", { class: `space ${cls}` }, left, middle, right));
}

function info(id: string) {
  return State.integrations[id];
}

function dataOf(id: string): Record<string, unknown> {
  return (info(id)?.data ?? {}) as Record<string, unknown>;
}

/** Not signed in (or the pill left the header): one line in the middle tile. */
function signedOut(id: string): boolean {
  return !(info(id)?.configured ?? false);
}

const time = (d: Date) => d.toLocaleTimeString(language(), { hour: "2-digit", minute: "2-digit" });

/** "In 25 min", "In 2 h 10". */
export function untilLabel(ms: number): string {
  const minutes = Math.max(1, Math.round(ms / 60_000));
  if (minutes < 60) return t("In {n} min", { n: minutes });
  return t("In {h} h {m}", { h: Math.floor(minutes / 60), m: String(minutes % 60).padStart(2, "0") });
}

/** When the space is open, a text field takes the keyboard; leaving it gives it back. */
function bindField(input: HTMLInputElement, actions: ViewActions) {
  input.addEventListener("focus", () => actions.keyboard(true));
  input.addEventListener("blur", () => actions.keyboard(false));
  input.addEventListener("input", () => actions.holdOpen(input.value.trim() !== ""));
}

/** The keyboard goes back when the island leaves the view. */
function releaseOnLeave(view: IslandViewName, input: HTMLInputElement, actions: ViewActions) {
  State.subscribe(() => {
    if ((State.view !== view || State.mode !== "expanded") && document.activeElement === input) {
      input.blur();
      actions.holdOpen(false);
    }
  });
}

/** The pill left the header while its space was on screen: back to the overview. */
function leftHeader(view: IslandViewName, id: string, actions: ViewActions): boolean {
  if (State.headerIds.includes(id)) return false;
  window.setTimeout(() => {
    if (State.view === view && !State.headerIds.includes(id)) actions.setView(State.defaultView());
  }, 0);
  return true;
}

// ── Calendar ──────────────────────────────────────────────────────────────────

/** What the left tile says: under way, in 25 min, at 14:30 tomorrow, nothing. */
export function nextLine(bookings: Booking[], now: number): { text: string; event: Booking | null } {
  const next = bookings.find((b) => !b.allDay && b.end.getTime() > now);
  if (!next) return { text: t("Nothing else today"), event: null };
  if (next.start.getTime() <= now) return { text: t("Now"), event: next };
  if (dayKey(next.start) !== dayKey(new Date(now))) return { text: t("Nothing else today"), event: next };
  return { text: untilLabel(next.start.getTime() - now), event: next };
}

/** Monday of the week holding `day`. */
function weekStart(day: Date): Date {
  return new Date(day.getFullYear(), day.getMonth(), day.getDate() - ((day.getDay() + 6) % 7));
}

export function buildAgendaSpace(actions: ViewActions): ViewHost {
  const left = tile("sp-date");
  const middle = tile("sp-week");
  const right = tile("sp-event");
  const el = spaceView("agenda", left, middle, right);
  let selectedDay = dayKey(new Date());
  /** Weeks from this one: ‹ and › move it, within the weeks the calendars were read for. */
  let weekOffset = 0;
  /** The island left the space: it opens on today the next time. */
  let away = false;
  State.subscribe(() => {
    if (State.view !== "agenda" || State.mode !== "expanded") away = true;
  });
  let selectedEvent: string | null = null;
  let key = "";

  function draw() {
    const now = Date.now();
    const today = new Date(now);
    const data = dataOf(CALENDAR_ID);
    const bookings = readBookings(data.bookings) ?? [];
    const calendars = readCalendars(data.calendars);
    const colorOf = (b: Booking, i: number) => calendars.find((c) => c.id === b.calendarId)?.color ?? EVENT_COLORS[i % EVENT_COLORS.length];
    const k = JSON.stringify([language(), Math.floor(now / 60_000), calendars, weekOffset, selectedDay, selectedEvent, signedOut(CALENDAR_ID),
      info(CALENDAR_ID)?.error ?? "", bookings.map((b) => [b.id, b.start.getTime(), b.end.getTime(), b.title])]);
    if (k === key) return;
    key = k;

    // Left: the date, and how long until the next event.
    clear(left);
    const next = nextLine(bookings, now);
    left.append(
      h("span", { class: "sp-kicker", text: today.toLocaleDateString(language(), { weekday: "long" }) }),
      h("span", { class: "sp-big", text: String(today.getDate()) }),
      h("span", { class: "sp-dim", text: today.toLocaleDateString(language(), { month: "long" }) }),
      h("span", { class: "sp-next", text: next.text }),
    );

    // Middle: this week, and the selected day's events.
    clear(middle);
    const thisMonday = weekStart(today);
    const monday = new Date(thisMonday.getFullYear(), thisMonday.getMonth(), thisMonday.getDate() + weekOffset * 7);
    const days = Array.from({ length: 7 }, (_, i) => new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + i));
    const range = `${days[0].toLocaleDateString(language(), { day: "numeric", month: "short" })} – ${days[6].toLocaleDateString(language(), { day: "numeric", month: "short" })}`;
    // The weeks the calendars were read for (calendar365.rs: a few weeks back, two months ahead).
    const from = Number(data.rangeStart), until = Number(data.rangeEnd);
    const canBack = !Number.isFinite(from) || monday.getTime() > from;
    const canOn = !Number.isFinite(until) || days[6].getTime() + 86_400_000 < until;
    const move = (by: number) => {
      actions.blip();
      weekOffset += by;
      const m = new Date(thisMonday.getFullYear(), thisMonday.getMonth(), thisMonday.getDate() + weekOffset * 7);
      selectedDay = weekOffset === 0 ? dayKey(new Date()) : dayKey(m);
      selectedEvent = null;
      draw();
    };
    const prev = h("button", { class: "sp-icon", title: t("Previous week"), "aria-label": t("Previous week"), onclick: () => move(-1) }, svg(ICONS.chevronLeft, 10, { stroke: 2.4 })) as HTMLButtonElement;
    const nextWeek = h("button", { class: "sp-icon", title: t("Next week"), "aria-label": t("Next week"), onclick: () => move(1) }, svg(ICONS.chevronRight, 10, { stroke: 2.4 })) as HTMLButtonElement;
    prev.disabled = !canBack;
    nextWeek.disabled = !canOn;
    middle.append(h("div", { class: "sp-title sp-week-nav" },
      h("b", { text: weekOffset === 0 ? t("This week") : range }),
      weekOffset === 0 ? h("span", { text: range }) : h("button", { class: "link-btn", text: t("Today"), onclick: () => move(-weekOffset) }),
      h("span", { class: "sp-spacer" }),
      prev, nextWeek));
    if (signedOut(CALENDAR_ID)) {
      middle.append(h("div", { class: "sp-empty", text: t("Sign in to Microsoft 365 in Settings.") }));
      clear(right);
      return;
    }
    const strip = h("div", { class: "sp-days" });
    for (const day of days) {
      const dk = dayKey(day);
      const count = bookings.filter((b) => dayKey(b.start) === dk).length;
      const cls = ["sp-day", dk === selectedDay ? "on" : "", dk === dayKey(today) ? "today" : "", count ? "busy" : ""].filter(Boolean).join(" ");
      strip.append(h("button", {
        class: cls,
        "data-day": dk,
        "aria-label": `${day.toLocaleDateString(language(), { weekday: "long", day: "numeric", month: "long" })} · ${count}`,
        onclick: () => {
          actions.blip();
          selectedDay = dk;
          selectedEvent = null;
          draw();
        },
      }, h("small", { text: day.toLocaleDateString(language(), { weekday: "short" }) }), h("span", { text: String(day.getDate()) })));
    }
    middle.append(strip);
    const list = h("div", { class: "sp-list" });
    const onDay = bookings.filter((b) => dayKey(b.start) === selectedDay);
    if (onDay.length === 0) list.append(h("div", { class: "sp-empty", text: t("No events") }));
    onDay.forEach((b, i) => {
      const past = b.end.getTime() <= now;
      list.append(h("button", {
        class: ["sp-event-row", b.id === selectedEvent ? "on" : "", past ? "past" : ""].filter(Boolean).join(" "),
        "data-event": b.id,
        onclick: () => {
          actions.blip();
          selectedEvent = b.id;
          draw();
        },
      },
      h("i", { style: `background:${colorOf(b, i)}` }),
      h("span", { class: "sp-time", text: b.allDay ? t("All day") : time(b.start) }),
      h("span", { class: "sp-name", text: b.title || t("Meeting") }),
      b.joinUrl ? h("span", { class: "sp-tag", text: "Teams" }) : null));
    });
    middle.append(list);

    // Right: the selected event, or the next one.
    clear(right);
    const shown = bookings.find((b) => b.id === selectedEvent) ?? next.event;
    right.append(h("span", { class: "sp-kicker dim", text: selectedEvent ? t("Meeting") : t("Next meeting") }));
    if (!shown) {
      right.append(h("div", { class: "sp-empty", text: t("No events") }));
      return;
    }
    const sameDay = dayKey(shown.start) === dayKey(today);
    const when = shown.allDay ? t("All day") : `${time(shown.start)} – ${time(shown.end)}`;
    right.append(
      h("h3", { class: "sp-event-title", text: shown.title || t("Meeting") }),
      h("div", { class: "sp-lines" },
        sameDay ? null : h("span", { text: shown.start.toLocaleDateString(language(), { weekday: "long", day: "numeric", month: "long" }) }),
        h("span", { text: when }),
        shown.attendeeName ? h("span", { class: "sp-dim", text: shown.attendeeName }) : null),
    );
    const buttons = h("div", { class: "sp-buttons" });
    if (shown.joinUrl) {
      const url = shown.joinUrl;
      buttons.append(h("button", { class: "sp-btn primary sp-join", text: t("Join the meeting"), onclick: () => void Bridge.openUrl(url) }));
    }
    if (shown.webUrl) {
      const url = shown.webUrl;
      buttons.append(h("button", { class: "sp-btn", text: t("Open in Outlook"), onclick: () => void Bridge.openUrl(url) }));
    }
    right.append(buttons);
  }

  return {
    el,
    sync() {
      if (State.view !== "agenda") return;
      if (away) {
        // Back on today each time the space opens.
        away = false;
        weekOffset = 0;
        selectedDay = dayKey(new Date());
        selectedEvent = null;
        key = "";
      }
      if (leftHeader("agenda", CALENDAR_ID, actions)) return;
      draw();
    },
  };
}

// ── Mail ──────────────────────────────────────────────────────────────────────

function mailData(): MailCardData {
  const d = dataOf(MAIL_ID) as Partial<MailCardData>;
  return {
    running: d.running ?? false,
    windows: d.windows ?? true,
    unread: d.unread ?? 0,
    messages: Array.isArray(d.messages) ? d.messages : [],
    shown: d.shown ?? 12,
    maxShown: d.maxShown ?? 60,
  };
}

export function buildMailSpace(actions: ViewActions): ViewHost {
  const left = tile("sp-count");
  const field = h("input", { class: "sp-search", type: "text", spellcheck: "false", placeholder: t("from:anne invoice") }) as HTMLInputElement;
  const filters = h("div", { class: "sp-filters" });
  const rows = h("div", { class: "sp-list" });
  // The inbox, or the message clicked in its place (with a way back).
  const listPane = h("div", { class: "sp-pane" },
    h("label", { class: "sp-search-box" }, svg(ICONS.magnifier, 12), field),
    filters, rows);
  const readPane = h("div", { class: "sp-pane sp-read" });
  const middle = tile("sp-mails", listPane, readPane);
  const el = spaceView("inbox", left, middle, null);

  let filter: MailFilter = "all";
  let results: MailMessage[] | null = null;
  let searching = false;
  let token = 0;
  let selected: MailMessage | null = null;
  const previews = new Map<string, { text: string; attachments: string[] } | null>();
  let listKey = "";
  let refreshed = 0;

  /** Deleted from here: gone from the list before the mailbox's next read says so. */
  const hidden = new Set<string>();
  const displayed = (): MailMessage[] => (results ?? mailData().messages).filter((m) => !hidden.has(messageId(m)));

  async function runSearch() {
    const q = field.value.trim();
    if (!q && filter === "all") {
      token++;
      results = null;
      searching = false;
      listKey = "";
      draw();
      return;
    }
    const mine = ++token;
    searching = true;
    listKey = "";
    draw();
    const found = await MailBridge.search(q, filter);
    if (mine !== token) return;
    searching = false;
    results = found ?? [];
    listKey = "";
    draw();
  }

  bindField(field, actions);
  releaseOnLeave("inbox", field, actions);
  field.addEventListener("keydown", (e) => {
    const k = (e as KeyboardEvent).key;
    if (k === "Enter") {
      e.preventDefault();
      void runSearch();
    } else if (k === "Escape" && field.value) {
      e.preventDefault();
      field.value = "";
      actions.holdOpen(false);
      void runSearch();
    } else if (k === "Escape") {
      return;
    }
    e.stopPropagation();
  });

  function select(m: MailMessage) {
    actions.blip();
    selected = m;
    const id = messageId(m);
    if (!previews.has(id)) {
      previews.set(id, null);
      void MailBridge.detail(m).then((d) => {
        previews.set(id, d ?? { text: "", attachments: [] });
        listKey = "";
        draw();
      });
    }
    listKey = "";
    draw();
  }

  async function act(m: MailMessage, kind: "read" | "flag" | "unflag" | "delete") {
    // Shown at once; the mailbox catches up.
    const id = messageId(m);
    if (kind === "delete") {
      // To Deleted Items; the message leaves the list and the inbox comes back.
      if (results) results = results.filter((x) => messageId(x) !== id);
      hidden.add(id);
      selected = null;
      listKey = "";
      draw();
      await MailBridge.act(m, "delete");
      return;
    }
    const change = (x: MailMessage): MailMessage => messageId(x) !== id ? x : {
      ...x,
      unread: kind === "read" ? false : x.unread,
      flagged: kind === "flag" ? true : kind === "unflag" ? false : x.flagged,
    };
    if (results) results = results.map(change);
    if (selected) selected = change(selected);
    listKey = "";
    draw();
    await MailBridge.act(m, kind);
  }

  function draw() {
    const d = mailData();
    const list = displayed();
    if (selected) {
      const fresh = list.find((x) => messageId(x) === messageId(selected!));
      if (fresh) selected = fresh;
    }
    const k = [
      language(), d.running, d.windows, d.unread, filter, searching, results === null, d.shown,
      Math.floor(Date.now() / 60_000), selected ? `${messageId(selected)}|${selected.unread}|${selected.flagged}` : "",
      selected ? previews.get(messageId(selected))?.text.length ?? -1 : -2,
      ...list.map((m) => `${messageId(m)}|${m.unread}|${m.flagged}`),
    ].join("~");
    if (k === listKey) return;
    listKey = k;

    clear(left);
    left.append(
      h("span", { class: "sp-kicker", text: t("Inbox") }),
      h("span", { class: "sp-big", text: d.unread > 999 ? "999+" : String(d.unread) }),
      h("span", { class: "sp-dim", text: t("Unread") }),
    );

    clear(filters);
    for (const f of MAIL_FILTERS) {
      filters.append(h("button", {
        class: f === filter ? "sp-chip on" : "sp-chip",
        text: filterTitle(f),
        onclick: () => {
          actions.blip();
          filter = f;
          void runSearch();
        },
      }));
    }
    if (searching) filters.append(h("span", { class: "mail-spinner" }));

    clear(rows);
    if (!d.running) {
      rows.append(h("div", { class: "sp-empty", text: d.windows
        ? t("Sign in to Microsoft 365 in Settings, or open the classic Outlook, to see your mail here.")
        : t("Sign in to Microsoft 365 in Settings to see your mail here.") }));
    } else if (list.length === 0) {
      rows.append(h("div", { class: "sp-empty", text: results !== null ? t("Nothing found") : t("No messages") }));
    } else {
      for (const m of list) {
        const on = selected !== null && messageId(m) === messageId(selected);
        rows.append(h("button", {
          class: ["sp-mail-row", m.unread ? "unread" : "", on ? "on" : ""].filter(Boolean).join(" "),
          "data-mail": messageId(m),
          onclick: () => select(m),
        },
        h("i", { class: m.unread ? "mail-dot on" : "mail-dot" }),
        h("span", { class: "sp-from", text: senderName(m) }),
        m.flagged ? h("span", { class: "mail-flag", text: "⚑" }) : null,
        h("span", { class: "sp-ago", text: timeAgo(m.date) }),
        h("span", { class: "sp-subject", text: m.subject })));
      }
      if (results === null && d.messages.length >= d.shown && d.shown < d.maxShown) {
        rows.append(h("button", { class: "link-btn mail-more", text: t("Load more"), onclick: () => void MailBridge.more() }));
      }
    }

    clear(readPane);
    const m = selected;
    middle.classList.toggle("reading", m !== null);
    if (!m) return;
    const preview = previews.get(messageId(m));
    const back = h("button", { class: "sp-back", title: t("Inbox"), onclick: () => backToInbox() },
      svg(ICONS.chevronLeft, 10, { stroke: 2.4 }), h("span", { text: t("Inbox") }));
    readPane.append(
      h("div", { class: "sp-mail-head" },
        back,
        h("b", { class: "sp-from", text: senderName(m), title: m.address }),
        h("span", { class: "sp-ago", text: timeAgo(m.date) })),
      h("div", { class: "sp-mail-subject", text: m.subject || " " }),
      h("div", { class: "sp-mail-text", text: preview == null ? t("Loading…") : preview.text.trim() || " " }),
    );
    // Reply opens Outlook's own window (or the draft on the web): nothing is sent from Ronron.
    const buttons = h("div", { class: "sp-buttons row" },
      h("button", { class: "sp-btn primary", text: t("Reply"), onclick: () => void MailBridge.open(m, true) }),
      h("button", { class: "sp-btn", text: t("Open"), onclick: () => void MailBridge.open(m, false) }),
    );
    if (m.unread) buttons.append(h("button", { class: "sp-btn", text: t("Mark as read"), onclick: () => void act(m, "read") }));
    buttons.append(h("button", {
      class: m.flagged ? "sp-btn amber on" : "sp-btn amber",
      text: m.flagged ? t("Not important") : t("Important"),
      onclick: () => void act(m, m.flagged ? "unflag" : "flag"),
    }));
    buttons.append(h("button", { class: "sp-btn danger", title: t("Moved to Deleted Items"), text: t("Delete"), onclick: () => void act(m, "delete") }));
    readPane.append(buttons);
  }

  function backToInbox() {
    actions.blip();
    selected = null;
    listKey = "";
    draw();
  }

  // Escape goes back to the inbox before it closes the island.
  window.addEventListener?.("keydown", (e) => {
    if (e.key !== "Escape" || selected === null || State.view !== "inbox" || State.mode !== "expanded") return;
    if (document.activeElement === field) return;
    e.stopImmediatePropagation();
    backToInbox();
  }, true);

  return {
    el,
    sync() {
      if (State.view !== "inbox") {
        // The space opens on the inbox again.
        if (selected) {
          selected = null;
          listKey = "";
        }
        return;
      }
      if (leftHeader("inbox", MAIL_ID, actions)) return;
      draw();
      // Opening the space reads the mailbox again (at most every 15 s).
      if (State.mode === "expanded" && Date.now() - refreshed > 15_000) {
        refreshed = Date.now();
        void MailBridge.refresh();
      }
    },
  };
}

// ── To Do ─────────────────────────────────────────────────────────────────────

const RING_R = 34;
const RING_C = 2 * Math.PI * RING_R;
const SVG_NS = "http://www.w3.org/2000/svg";

/** The progress ring: a grey track and the orange share done. */
function ringSvg(share: number): SVGElement {
  const root = document.createElementNS(SVG_NS, "svg");
  for (const [k, v] of Object.entries({ width: "84", height: "84", viewBox: "0 0 84 84", "aria-hidden": "true" })) root.setAttribute(k, v);
  const circle = (stroke: string, dash: string | null) => {
    const c = document.createElementNS(SVG_NS, "circle");
    for (const [k, v] of Object.entries({ cx: "42", cy: "42", r: String(RING_R), fill: "none", stroke, "stroke-width": "7", "stroke-linecap": "round" })) c.setAttribute(k, v);
    if (dash !== null) {
      c.setAttribute("stroke-dasharray", dash);
      c.setAttribute("transform", "rotate(-90 42 42)");
    }
    root.append(c);
  };
  circle("#2A2C30", null);
  if (share > 0) circle(ACCENT, `${(share * RING_C).toFixed(1)} ${RING_C.toFixed(1)}`);
  return root;
}

export function buildTodoSpace(actions: ViewActions): ViewHost {
  const left = tile("sp-progress");
  const head = h("div", { class: "sp-title" });
  const rows = h("div", { class: "sp-list" });
  const input = h("input", { class: "sp-add-input", type: "text", maxlength: "255", spellcheck: "true", placeholder: t("Add a task") }) as HTMLInputElement;
  const addBtn = h("button", { class: "sp-add-go", title: t("Add a task"), "aria-label": t("Add a task") }, svg(ICONS.plus, 12, { stroke: 2.2 }));
  const error = h("div", { class: "sp-error" });
  const menu = h("div", { class: "sp-menu", role: "listbox" });
  const middle = tile("sp-tasks", head, menu, rows, error, h("label", { class: "sp-add" }, input, addBtn));
  const el = spaceView("tasks", left, middle, null);
  /** The list shown in the middle; null = today's (late and due today). */
  let listFilter: string | null = null;
  /** The list picker under the title is open. */
  let picking = false;
  let key = "";

  async function submit() {
    const title = input.value;
    if (!title.trim() || todoActions.busy()) return;
    const lists = listsOf();
    const target = listFilter ?? lists.find((l) => l.default)?.id ?? null;
    if (await todoActions.add(title, target)) {
      input.value = "";
      actions.holdOpen(false);
    }
  }

  bindField(input, actions);
  releaseOnLeave("tasks", input, actions);
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
  addBtn.addEventListener("click", () => void submit());

  function listsOf(): { id: string; name: string; default: boolean }[] {
    const raw = dataOf(TODO_ID).lists;
    if (!Array.isArray(raw)) return [];
    return raw.filter((l) => l && typeof l.id === "string").map((l) => ({
      id: l.id as string, name: typeof l.name === "string" ? l.name : "", default: l.default === true,
    }));
  }

  function row(task: TodoTask, today: string): HTMLElement {
    const done = todoActions.isDone(task.id);
    const due = dueLabel(task.due, today);
    const check = h("button", {
      class: done ? "sp-check on" : "sp-check",
      title: done ? t("Undo") : t("Mark as done"),
      "aria-label": done ? t("Undo") : t("Mark as done"),
      onclick: () => {
        actions.blip();
        if (done) todoActions.undo(task);
        else todoActions.complete(task);
      },
    });
    if (done) check.append(svg(ICONS.check, 9, { stroke: 2.6 }));
    return h("div", { class: done ? "sp-task done" : "sp-task", "data-task": task.id },
      check,
      h("span", { class: "sp-name", text: task.title }),
      task.importance === "high" && !done ? h("span", { class: "todo-flag", text: "!" }) : null,
      done
        ? h("button", { class: "link-btn todo-undo", text: t("Undo"), onclick: () => todoActions.undo(task) })
        : due ? h("span", { class: due.overdue ? "sp-due late" : "sp-due", text: due.text })
        : listFilter === null && task.list ? h("span", { class: "sp-due", text: task.list }) : null);
  }

  function draw() {
    const today = todayKey();
    const open = todoActions.open();
    const done = todoActions.justDone();
    const lists = listsOf();
    if (listFilter && !lists.some((l) => l.id === listFilter)) listFilter = null;
    const k = JSON.stringify([language(), today, listFilter, picking, signedOut(TODO_ID), todoActions.error(), todoActions.busy(),
      doneTodayCount(today), lists, open.map((x) => [x.id, x.title, x.due, x.importance, x.list]), done.map((x) => x.id)]);
    if (k === key) return;
    key = k;

    // Left: today's progress — ticked off in Ronron today, out of that plus what is still due.
    const dueNow = open.filter((x) => x.due !== null && x.due <= today);
    const doneCount = doneTodayCount(today);
    const total = doneCount + dueNow.length;
    const late = overdueCount(open, today);
    const share = total ? doneCount / total : 0;
    clear(left);
    const ring = h("div", { class: "sp-ring" }, ringSvg(share));
    ring.append(h("div", { class: "sp-ring-text" }, h("b", { text: `${doneCount}/${total}` }), h("small", { text: t("Today") })));
    left.append(ring, h("span", { class: late ? "sp-late" : "sp-dim", text: late ? t("Overdue: {n}", { n: late }) : t("Tasks") + ` · ${open.length}` }));

    // Middle: today's tasks, or a list's. On the right of the title, the list shown and its
    // count: a click opens the other lists (Today first).
    const counts = new Map<string, number>();
    for (const x of open) counts.set(x.listId, (counts.get(x.listId) ?? 0) + 1);
    const current = lists.find((l) => l.id === listFilter);
    const label = (name: string, n: number) => `${name} - ${n}`;
    const choices: { id: string | null; text: string }[] = [
      { id: null, text: label(t("Today"), dueNow.length) },
      ...lists.map((l) => ({ id: l.id, text: label(l.name || t("Tasks"), counts.get(l.id) ?? 0) })),
    ];
    const shownChoice = choices.find((c) => c.id === (current ? current.id : null)) ?? choices[0];
    clear(head);
    const picker = h("button", {
      class: picking ? "sp-picker on" : "sp-picker",
      "aria-haspopup": "listbox",
      "aria-expanded": picking ? "true" : "false",
      onclick: () => {
        actions.blip();
        picking = !picking;
        key = "";
        draw();
      },
    }, h("span", { text: shownChoice.text }), svg(ICONS.chevronDown, 9, { stroke: 2.2 }));
    head.append(h("b", { text: t("Tasks") }), h("span", { class: "sp-spacer" }), picker,
      h("button", { class: "sp-icon", title: t("Open {name}", { name: "To Do" }), onclick: () => void Bridge.openUrl(todoActions.webUrl()) },
        svg(ICONS.arrowUpRight, 11)));
    menu.style.display = picking ? "" : "none";
    clear(menu);
    if (picking) {
      for (const c of choices) {
        menu.append(h("button", {
          class: c.id === (current ? current.id : null) ? "sp-menu-row on" : "sp-menu-row",
          role: "option",
          "data-list": c.id ?? "",
          text: c.text,
          onclick: () => {
            actions.blip();
            listFilter = c.id;
            picking = false;
            key = "";
            draw();
          },
        }));
      }
    }
    clear(rows);
    if (signedOut(TODO_ID)) {
      rows.append(h("div", { class: "sp-empty", text: t("Sign in to Microsoft 365 in Settings.") }));
    } else {
      const shown = listFilter ? open.filter((x) => x.listId === listFilter) : dueNow.length ? dueNow : open;
      const doneHere = done.filter((x) => !listFilter || x.listId === listFilter);
      for (const task of doneHere) rows.append(row(task, today));
      for (const task of shown) rows.append(row(task, today));
      if (shown.length === 0 && doneHere.length === 0) rows.append(h("div", { class: "sp-empty", text: t("No tasks left") }));
    }
    error.textContent = todoActions.error() ?? "";
    input.disabled = todoActions.busy() || signedOut(TODO_ID);

  }

  return {
    el,
    sync() {
      if (State.view !== "tasks") {
        if (picking) {
          picking = false;
          key = "";
        }
        return;
      }
      if (leftHeader("tasks", TODO_ID, actions)) return;
      draw();
    },
  };
}
