// Microsoft To Do: the card in the overview and its detail (src-tauri/src/todo).
// The card shows what is due first; ○ ticks a task off in To Do, with Undo for a
// few seconds; + adds a task to the default list.

import { h, svg, clear, dot } from "./dom";
import { ICONS } from "./icons";
import { State } from "../core/state";
import { Bridge } from "../core/bridge";
import { language, t } from "../i18n/i18n";

export const TODO_ID = "integration_todo";
export const TODO_COLOR = "#2564CF";
const UNDO_MS = 6000;
const CARD_ROWS = 3;

export interface TodoTask {
  id: string;
  listId: string;
  list: string;
  title: string;
  importance: string;
  /** "YYYY-MM-DD" or null. */
  due: string | null;
}

/** The tasks in the integration data, with anything malformed left out. */
export function readTasks(data: unknown): TodoTask[] {
  const raw = (data as { tasks?: unknown } | null)?.tasks;
  if (!Array.isArray(raw)) return [];
  const out: TodoTask[] = [];
  for (const item of raw) {
    if (!item || typeof item !== "object") continue;
    const r = item as Record<string, unknown>;
    if (typeof r.id !== "string" || typeof r.listId !== "string") continue;
    out.push({
      id: r.id,
      listId: r.listId,
      list: typeof r.list === "string" ? r.list : "",
      title: typeof r.title === "string" && r.title.trim() ? r.title : t("Untitled"),
      importance: typeof r.importance === "string" ? r.importance : "normal",
      due: typeof r.due === "string" && /^\d{4}-\d{2}-\d{2}$/.test(r.due) ? r.due : null,
    });
  }
  return out;
}

/** Today in local time, as "YYYY-MM-DD". */
export function todayKey(now = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;
}

function shiftDay(day: string, by: number): string {
  const [y, m, d] = day.split("-").map(Number);
  return todayKey(new Date(y, m - 1, d + by));
}

/** Dated tasks first, earliest due first (overdue on top); then the important ones; then the rest. */
export function sortTasks(tasks: TodoTask[]): TodoTask[] {
  const weight = (x: TodoTask) => (x.importance === "high" ? 0 : 1);
  return tasks.slice().sort((a, b) => {
    if (a.due && b.due && a.due !== b.due) return a.due < b.due ? -1 : 1;
    if (a.due && !b.due) return -1;
    if (!a.due && b.due) return 1;
    return weight(a) - weight(b);
  });
}

export function overdueCount(tasks: TodoTask[], today = todayKey()): number {
  return tasks.filter((x) => x.due !== null && x.due < today).length;
}

/** "Yesterday", "Today", "Tomorrow" or a short date; red when it is past. */
export function dueLabel(due: string | null, today = todayKey()): { text: string; overdue: boolean } | null {
  if (!due) return null;
  const overdue = due < today;
  if (due === today) return { text: t("Today"), overdue };
  if (due === shiftDay(today, -1)) return { text: t("Yesterday"), overdue };
  if (due === shiftDay(today, 1)) return { text: t("Tomorrow"), overdue };
  const [y, m, d] = due.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  const sameYear = y === Number(today.slice(0, 4));
  return {
    text: date.toLocaleDateString(language(), sameYear ? { weekday: "short", day: "numeric", month: "short" } : { day: "numeric", month: "short", year: "numeric" }),
    overdue,
  };
}

// ── What the user is doing in the card ───────────────────────────────────────

/** Tasks just ticked off: shown struck through, with Undo, for a few seconds. */
const justDone = new Map<string, { task: TodoTask; timer: number }>();
let adding = false;
let draft = "";
let busy = false;
let lastError: string | null = null;

function data(): Record<string, unknown> {
  return (State.integrations[TODO_ID]?.data ?? {}) as Record<string, unknown>;
}

/** The open tasks, sorted, without the ones being undone-or-not right now. */
function openTasks(): TodoTask[] {
  return sortTasks(readTasks(data()).filter((x) => !justDone.has(x.id)));
}

function fail(error: unknown) {
  lastError = error instanceof Error ? error.message : String(error ?? t("Not available"));
  State.notify();
}

// Ticked off in Ronron today, for the To Do space's ring (this computer only).
const DONE_KEY = "ronron.todo.doneToday";
let doneToday: { day: string; count: number } | null = null;

/** Tasks ticked off from Ronron today. */
export function doneTodayCount(today = todayKey()): number {
  if (!doneToday) {
    try {
      const saved = JSON.parse(window.localStorage.getItem(DONE_KEY) ?? "null");
      if (saved && typeof saved.day === "string" && typeof saved.count === "number") doneToday = saved;
    } catch {
      // No storage here: counted for this run only.
    }
  }
  return doneToday?.day === today ? doneToday.count : 0;
}

function countDone(by: number) {
  const day = todayKey();
  doneToday = { day, count: Math.max(0, doneTodayCount(day) + by) };
  try {
    window.localStorage.setItem(DONE_KEY, JSON.stringify(doneToday));
  } catch {
    // Kept in memory only.
  }
}

async function complete(task: TodoTask) {
  lastError = null;
  countDone(1);
  const timer = window.setTimeout(() => {
    justDone.delete(task.id);
    State.notify();
  }, UNDO_MS);
  justDone.set(task.id, { task, timer });
  State.notify();
  try {
    await Bridge.todoSetDone(task.listId, task.id, true);
  } catch (error) {
    window.clearTimeout(timer);
    justDone.delete(task.id);
    countDone(-1);
    fail(error);
  }
}

async function undo(task: TodoTask) {
  const entry = justDone.get(task.id);
  if (entry) window.clearTimeout(entry.timer);
  justDone.delete(task.id);
  countDone(-1);
  State.notify();
  try {
    await Bridge.todoSetDone(task.listId, task.id, false);
  } catch (error) {
    fail(error);
  }
}

async function add(title: string, listId: string | null = null): Promise<boolean> {
  if (!title.trim() || busy) return false;
  let ok = false;
  busy = true;
  lastError = null;
  State.notify();
  try {
    await Bridge.todoAdd(title.trim(), listId);
    draft = "";
    adding = false;
    ok = true;
  } catch (error) {
    lastError = error instanceof Error ? error.message : String(error);
  } finally {
    busy = false;
    State.notify();
  }
  return ok;
}

// ── For the To Do space (spaces.ts) ──────────────────────────────────────────

export const todoActions = {
  complete: (task: TodoTask) => void complete(task),
  undo: (task: TodoTask) => void undo(task),
  add: (title: string, listId: string | null) => add(title, listId),
  isDone: (id: string) => justDone.has(id),
  justDone: (): TodoTask[] => [...justDone.values()].map((d) => d.task),
  open: (): TodoTask[] => openTasks(),
  busy: () => busy,
  error: () => lastError,
  webUrl: () => webUrl(),
};

// ── Rows ──────────────────────────────────────────────────────────────────────

function taskRow(task: TodoTask, showList: boolean): HTMLElement {
  const done = justDone.has(task.id);
  const due = dueLabel(task.due);
  const check = h("button", {
    class: done ? "todo-check on" : "todo-check",
    title: done ? t("Undo") : t("Mark as done"),
    "aria-label": done ? t("Undo") : t("Mark as done"),
    onclick: (e: Event) => {
      e.stopPropagation();
      void (done ? undo(task) : complete(task));
    },
  });
  if (done) check.append(svg(ICONS.check, 8, { stroke: 2.6 }));
  const row = h("div", { class: done ? "int-row todo-row done" : "todo-row int-row" }, check,
    h("span", { class: "int-name", text: task.title }));
  if (task.importance === "high" && !done) row.append(h("span", { class: "todo-flag", text: "!" }));
  if (done) {
    row.append(h("button", { class: "link-btn todo-undo", text: t("Undo"), onclick: () => void undo(task) }));
  } else if (due) {
    row.append(h("span", { class: due.overdue ? "int-time todo-late" : "int-time", text: due.text }));
  } else if (showList && task.list) {
    row.append(h("span", { class: "int-ago", text: task.list }));
  }
  return row;
}

function addRow(): HTMLElement {
  const input = h("input", {
    class: "todo-input",
    placeholder: t("Add a task"),
    maxlength: "255",
    spellcheck: "true",
  }) as HTMLInputElement;
  input.value = draft;
  input.disabled = busy;
  input.addEventListener("input", () => (draft = input.value));
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") void add(input.value);
    if (e.key === "Escape") {
      e.stopPropagation();
      adding = false;
      draft = "";
      State.notify();
    }
  });
  // The card is redrawn on every update: give the focus back.
  window.setTimeout(() => input.focus(), 0);
  return h("div", { class: "int-row todo-row" }, h("span", { class: "todo-check ghost" }), input);
}

function plusButton(): HTMLElement {
  return h("button", {
    class: "int-back todo-plus",
    title: t("Add a task"),
    "aria-label": t("Add a task"),
    onclick: () => {
      adding = !adding;
      State.notify();
    },
  }, svg(ICONS.plus, 10));
}

function webUrl(): string {
  const url = data().webUrl;
  return typeof url === "string" && url.startsWith("https://") ? url : "https://to-do.office.com/tasks/";
}

// ── The card and its detail ──────────────────────────────────────────────────

export function todoCard(openDetail: () => void): HTMLElement {
  const tasks = openTasks();
  const late = overdueCount(tasks);
  const kind = late > 0 ? t("Overdue: {n}", { n: late }) : t("Tasks");
  const head = h("div", { class: "int-head" },
    dot(TODO_COLOR, 7),
    h("b", { text: "To Do" }),
    h("span", { class: late > 0 ? "todo-late" : "", text: kind }),
    plusButton());
  const rows = h("div", { class: "int-rows tight" });
  if (adding) rows.append(addRow());
  const shown = [...[...justDone.values()].map((d) => d.task), ...tasks].slice(0, adding ? CARD_ROWS - 1 : CARD_ROWS);
  if (shown.length === 0 && !adding) rows.append(h("div", { class: "int-empty", text: t("No tasks left") }));
  for (const task of shown) rows.append(taskRow(task, false));
  if (lastError) rows.append(h("div", { class: "int-status todo-error", text: lastError }));
  const more = tasks.length > shown.length ? `${shown.length}/${tasks.length}` : "";
  return h("div", { class: "int-card todo-card" }, head, rows,
    h("div", { class: "int-actions" },
      more ? h("span", { class: "int-ago", text: more }) : h("span", {}),
      h("button", { class: "link-btn", text: t("Details"), onclick: openDetail })));
}

export function todoDetail(close: () => void): HTMLElement {
  const root = h("div", { class: "int-card detail todo-card" });
  const tasks = openTasks();
  root.append(h("div", { class: "int-detail-head" },
    h("button", { class: "int-back", onclick: close }, svg(ICONS.chevronLeft, 10, { stroke: 2.4 })),
    dot(TODO_COLOR, 6),
    h("b", { text: "To Do" }),
    plusButton()));
  const body = h("div", { class: "int-detail-body" });
  if (adding) body.append(addRow());
  for (const d of justDone.values()) body.append(taskRow(d.task, true));
  // Grouped by list, in the order Graph gave the lists (the default one first).
  const lists = Array.isArray(data().lists) ? (data().lists as { id: string; name: string }[]) : [];
  const order = new Map(lists.map((l, i) => [l.id, i]));
  const groups = new Map<string, TodoTask[]>();
  for (const task of tasks) groups.set(task.listId, [...(groups.get(task.listId) ?? []), task]);
  const ids = [...groups.keys()].sort((a, b) => (order.get(a) ?? 99) - (order.get(b) ?? 99));
  for (const id of ids) {
    const list = groups.get(id) ?? [];
    body.append(h("div", { class: "todo-list-name", text: `${list[0]?.list || t("Tasks")} · ${list.length}` }));
    for (const task of list) body.append(taskRow(task, false));
  }
  if (tasks.length === 0 && justDone.size === 0) body.append(h("div", { class: "int-empty", text: t("No tasks left") }));
  if (lastError) body.append(h("div", { class: "int-status todo-error", text: lastError }));
  root.append(body, h("div", { class: "int-actions" },
    h("button", { class: "link-btn", text: t("Open {name}", { name: "To Do" }), onclick: () => void Bridge.openUrl(webUrl()) })));
  root.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !adding) {
      event.stopPropagation();
      close();
    }
  });
  return root;
}

/** For tests: forget what the card was doing. */
export function resetTodoView() {
  for (const d of justDone.values()) window.clearTimeout(d.timer);
  justDone.clear();
  adding = false;
  draft = "";
  busy = false;
  lastError = null;
  doneToday = null;
}

export { clear };
