// Microsoft To Do: reading Graph's tasks, the order and due labels, and the card's
// actions reaching Rust (src/views/todo.ts, src-tauri/src/todo).

import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { sent } from "./tauri.mjs";
import { State } from "../src/core/state.ts";
import { pillDefinition, availablePills } from "../src/core/pills.ts";
import { idleStatus, hasIntegrationData, renderIntegrationCard } from "../src/views/integrations.ts";
import {
  TODO_ID, dueLabel, overdueCount, readTasks, resetTodoView, sortTasks, todayKey, todoCard, todoDetail,
} from "../src/views/todo.ts";

installFakeDom();

const TODAY = "2026-11-20";
const task = (id, extra = {}) => ({ id, listId: "L1", list: "Tâches", title: id, importance: "normal", due: null, ...extra });

beforeEach(() => {
  resetTodoView();
  delete State.integrations[TODO_ID];
});

test("the pill is a service with a sign-in, on Windows and Linux", () => {
  const def = pillDefinition(TODO_ID);
  assert.equal(def.name, "To Do");
  assert.equal(def.category, "service");
  assert.deepEqual(def.connect, { kind: "key", key: "msgraph-refresh-0" });
  assert.ok(availablePills("windows").some((p) => p.id === TODO_ID));
  assert.ok(availablePills("linux").some((p) => p.id === TODO_ID));
  assert.equal(idleStatus(TODO_ID, { configured: false, error: null }, "").label, "Not signed in");
  assert.equal(idleStatus(TODO_ID, { configured: true, error: null, loaded: true }, "").label, "Connected");
});

test("malformed tasks are left out", () => {
  const tasks = readTasks({ tasks: [
    task("a", { due: "2026-11-21" }),
    { id: 3, listId: "L1" },
    { id: "b" },
    task("c", { due: "21/11/2026", title: "  " }),
    null,
  ] });
  assert.deepEqual(tasks.map((x) => x.id), ["a", "c"]);
  assert.equal(tasks[1].due, null);
  assert.equal(tasks[1].title, "Untitled");
  assert.deepEqual(readTasks({}), []);
  assert.deepEqual(readTasks(null), []);
});

test("overdue first, then by due day, then the important ones", () => {
  const sorted = sortTasks([
    task("someday"),
    task("important", { importance: "high" }),
    task("next week", { due: "2026-11-27" }),
    task("late", { due: "2026-11-18" }),
    task("today", { due: TODAY }),
  ]);
  assert.deepEqual(sorted.map((x) => x.id), ["late", "today", "next week", "important", "someday"]);
  assert.equal(overdueCount(sorted, TODAY), 1);
});

test("due labels", () => {
  assert.deepEqual(dueLabel(TODAY, TODAY), { text: "Today", overdue: false });
  assert.deepEqual(dueLabel("2026-11-19", TODAY), { text: "Yesterday", overdue: true });
  assert.deepEqual(dueLabel("2026-11-21", TODAY), { text: "Tomorrow", overdue: false });
  assert.equal(dueLabel("2026-11-10", TODAY).overdue, true);
  assert.equal(dueLabel(null, TODAY), null);
  // Across a month end.
  assert.equal(dueLabel("2026-12-01", "2026-11-30").text, "Tomorrow");
  assert.equal(todayKey(new Date(2026, 0, 5, 23, 59)), "2026-01-05");
});

function load(tasks) {
  State.integrations[TODO_ID] = {
    data: { tasks, lists: [{ id: "L1", name: "Tâches", default: true }, { id: "L2", name: "Courses", default: false }], webUrl: "https://to-do.office.com/tasks/" },
    error: null, loaded: true, configured: true,
  };
}

test("the card shows three tasks, what is late, and a way to the rest", () => {
  load([task("a", { due: "2000-01-01" }), task("b"), task("c"), task("d", { listId: "L2", list: "Courses" })]);
  assert.ok(hasIntegrationData(TODO_ID));
  const card = todoCard(() => {});
  assert.equal(card.find(".todo-row").length, 3);
  assert.match(card.textContent, /Overdue: 1/);
  assert.match(card.textContent, /3\/4/);
  const via = renderIntegrationCard({ id: TODO_ID, steps: [] }, { detailOpen: false, openDetail() {}, closeDetail() {}, openSettings() {} });
  assert.equal(via.find(".todo-row").length, 3);
});

test("ticking a task off reaches Graph, and Undo puts it back", async () => {
  load([task("a"), task("b")]);
  const card = todoCard(() => {});
  card.querySelector(".todo-check").fire("click");
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(sent("todo_set_done").at(-1), { listId: "L1", taskId: "a", done: true });
  const again = todoCard(() => {});
  assert.ok(again.querySelector(".todo-row.done"));
  again.querySelector(".todo-undo").fire("click");
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(sent("todo_set_done").at(-1), { listId: "L1", taskId: "a", done: false });
  assert.equal(todoCard(() => {}).querySelector(".todo-row.done"), null);
});

test("+ adds a task to the default list", async () => {
  load([]);
  todoCard(() => {}).querySelector(".todo-plus").fire("click");
  const card = todoCard(() => {});
  const input = card.querySelector(".todo-input");
  input.value = "  Relancer le devis  ";
  for (const fn of input.listeners.get("keydown")) fn({ key: "Enter", stopPropagation() {} });
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(sent("todo_add").at(-1), { title: "Relancer le devis", listId: null });
});

test("the detail groups tasks by list, the default list first", () => {
  load([task("x", { listId: "L2", list: "Courses" }), task("y"), task("z")]);
  const detail = todoDetail(() => {});
  const names = detail.find(".todo-list-name").map((el) => el.textContent);
  assert.deepEqual(names, ["Tâches · 2", "Courses · 1"]);
});
