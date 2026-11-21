// The Microsoft 365 spaces (src/views/spaces.ts): Calendar, Mail and To Do, each in
// its own view from the header tabs, their buttons reaching Rust.

import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { sent } from "./tauri.mjs";
import { State } from "../src/core/state.ts";
import { VIEW_LAYOUTS, islandSize } from "../src/core/layout.ts";
import { buildAgendaSpace, buildMailSpace, buildTodoSpace, nextLine, untilLabel } from "../src/views/spaces.ts";
import { resetTodoView, todayKey } from "../src/views/todo.ts";
import { readBookings } from "../src/core/calendar.ts";

installFakeDom();

const iso = (ms) => new Date(ms).toISOString();
const text = (el) => el.textContent;

function actions() {
  const did = { views: [], keyboard: [], hold: [] };
  return {
    did,
    blip() {},
    setView: (v) => did.views.push(v),
    keyboard: (on) => did.keyboard.push(on),
    holdOpen: (on) => did.hold.push(on),
  };
}

beforeEach(() => {
  resetTodoView();
  State.settings.headerPills = ["integration_calendar", "integration_mail", "integration_todo"];
});

test("the spaces are tall, without pills, Ronron in the left tile", () => {
  for (const view of ["agenda", "inbox", "tasks"]) {
    const layout = VIEW_LAYOUTS[view];
    assert.equal(layout.agentMode, "none");
    assert.ok(layout.botX < 150 && layout.botY > 200);
    assert.ok(islandSize("expanded", view).h <= 320);
  }
});

test("an open pill makes the overview as tall as a space", () => {
  assert.equal(islandSize("expanded", "overview").h, VIEW_LAYOUTS.overview.height);
  // The chat is as tall as the To Do space.
  assert.equal(islandSize("expanded", "prompt").h, islandSize("expanded", "tasks").h);
  assert.equal(islandSize("expanded", "overview", true).h, VIEW_LAYOUTS.agenda.height);
  assert.equal(islandSize("expanded", "launch", true).h, VIEW_LAYOUTS.launch.height);
});

test("how long until the next event", () => {
  assert.equal(untilLabel(25 * 60_000), "In 25 min");
  assert.equal(untilLabel(130 * 60_000), "In 2 h 10");
  const now = new Date(2026, 10, 20, 14, 5).getTime();
  const at = (h, m) => iso(new Date(2026, 10, 20, h, m).getTime());
  const bookings = readBookings([
    { id: "a", start: at(9, 0), end: at(10, 0) },
    { id: "b", start: at(14, 30), end: at(15, 0) },
  ]);
  assert.deepEqual([nextLine(bookings, now).text, nextLine(bookings, now).event.id], ["In 25 min", "b"]);
  assert.equal(nextLine(bookings, new Date(2026, 10, 20, 14, 40).getTime()).text, "Now");
  assert.equal(nextLine(bookings, new Date(2026, 10, 20, 16, 0).getTime()).text, "Nothing else today");
});

test("Calendar: the week, today's events, and the next one to join", () => {
  const now = Date.now();
  State.view = "agenda";
  State.integrations.integration_calendar = {
    data: { rangeStart: now - 86_400_000, rangeEnd: now + 30 * 86_400_000, bookings: [
      { id: "next", title: "Point client", start: iso(now + 600_000), end: iso(now + 1_800_000), joinUrl: "https://teams.microsoft.com/x", webUrl: "https://outlook.office365.com/y" },
    ] },
    error: null, loaded: true, configured: true,
  };
  const a = actions();
  const view = buildAgendaSpace(a);
  view.sync();
  assert.equal(view.el.find(".sp-day").length, 7);
  assert.match(text(view.el), /This week/);
  assert.match(text(view.el), /Next meeting/);
  assert.match(text(view.el), /Point client/);
  assert.doesNotMatch(text(view.el), /pill/i);
  view.el.querySelector(".sp-join").fire("click");
  assert.deepEqual(sent("open_url").at(-1), { url: "https://teams.microsoft.com/x" });

  // Week by week, inside the weeks read; Today comes back.
  const days = () => view.el.find(".sp-day").map((d) => d.getAttribute("data-day"));
  const thisWeek = days();
  const [prev, next] = view.el.find(".sp-icon");
  assert.equal(prev.disabled, true); // read from yesterday only
  next.fire("click");
  assert.notDeepEqual(days(), thisWeek);
  assert.doesNotMatch(text(view.el), /This week/);
  view.el.find(".link-btn").find((b) => b.textContent === "Today").fire("click");
  assert.deepEqual(days(), thisWeek);

  // Signed out: one line, no buttons.
  State.integrations.integration_calendar = { data: {}, error: null, loaded: true, configured: false };
  view.sync();
  assert.match(text(view.el), /Sign in to Microsoft 365 in Settings\./);
  assert.equal(view.el.querySelector(".sp-join"), null);
});

test("a space whose pill left the header goes back to the overview", async () => {
  State.view = "agenda";
  State.settings.headerPills = [];
  const a = actions();
  buildAgendaSpace(a).sync();
  await new Promise((r) => setTimeout(r, 5));
  assert.equal(a.did.views.length, 1);
});

test("Mail: the unread count, the list, and a message's actions", async () => {
  State.view = "inbox";
  State.mode = "expanded";
  const m = { entryId: "e1", storeId: "graph", account: "me", senderName: "Anne", address: "anne@x.ch", subject: "Facture", date: Date.now(), unread: true, flagged: false, local: "" };
  State.integrations.integration_mail = { data: { running: true, windows: true, unread: 3, messages: [m], shown: 12, maxShown: 60 }, error: null, loaded: true, configured: true };
  const a = actions();
  const view = buildMailSpace(a);
  view.sync();
  assert.ok(sent("mail_refresh").length > 0);
  assert.equal(view.el.querySelector(".sp-big").textContent, "3");
  assert.equal(view.el.querySelector(".reading"), null);
  view.el.querySelector(".sp-mail-row").fire("click");
  // The message takes the inbox's place.
  assert.ok(view.el.querySelector(".sp-mails").classList.contains("reading"));
  assert.ok(sent("mail_detail").length > 0);
  assert.match(text(view.el), /Facture/);
  const button = (label) => view.el.find(".sp-btn").find((b) => b.textContent === label);
  button("Mark as read").fire("click");
  assert.deepEqual(sent("mail_act").at(-1), { message: m, kind: "read" });
  // Reply opens the mail program: nothing is sent from Ronron.
  button("Reply").fire("click");
  assert.deepEqual(sent("mail_open").at(-1), { message: m, reply: true });
  // Back to the inbox.
  view.el.querySelector(".sp-back").fire("click");
  assert.ok(!view.el.querySelector(".sp-mails").classList.contains("reading"));
  assert.equal(view.el.find(".sp-mail-row").length, 1);
  // Delete: to Deleted Items, gone from the list, back on the inbox.
  view.el.querySelector(".sp-mail-row").fire("click");
  view.el.querySelector(".danger").fire("click");
  assert.deepEqual(sent("mail_act").at(-1), { message: m, kind: "delete" });
  assert.ok(!view.el.querySelector(".sp-mails").classList.contains("reading"));
  assert.equal(view.el.find(".sp-mail-row").length, 0);
});

test("To Do: today's tasks, ticking one off, and the list picker", () => {
  State.view = "tasks";
  const today = todayKey();
  const later = todayKey(new Date(Date.now() + 3 * 86_400_000));
  State.integrations.integration_todo = {
    data: {
      lists: [{ id: "L1", name: "Tâches", default: true }, { id: "L2", name: "Maison", default: false }],
      tasks: [
        { id: "late", listId: "L1", list: "Tâches", title: "Relancer le devis", importance: "high", due: "2000-01-01" },
        { id: "now", listId: "L1", list: "Tâches", title: "Certificat SSL", importance: "normal", due: today },
        { id: "soon", listId: "L2", list: "Maison", title: "Chlore piscine", importance: "normal", due: later },
      ],
    },
    error: null, loaded: true, configured: true,
  };
  const a = actions();
  const view = buildTodoSpace(a);
  view.sync();
  const rows = () => view.el.find(".sp-task").map((r) => r.getAttribute("data-task"));
  assert.deepEqual(rows(), ["late", "now"]);
  assert.match(text(view.el), /Overdue: 1/);
  assert.match(view.el.querySelector(".sp-ring-text").textContent, /0\/2/);
  assert.equal(view.el.querySelector(".sp-picker").textContent, "Today - 2");
  assert.equal(view.el.querySelector(".sp-soon"), null);

  view.el.find(".sp-check")[1].fire("click");
  assert.deepEqual(sent("todo_set_done").at(-1), { listId: "L1", taskId: "now", done: true });
  view.sync();
  assert.match(view.el.querySelector(".sp-ring-text").textContent, /1\/2/);

  // The picker under the title: Today, then each list with its count.
  view.el.querySelector(".sp-picker").fire("click");
  assert.deepEqual(view.el.find(".sp-menu-row").map((r) => r.textContent), ["Today - 1", "Tâches - 1", "Maison - 1"]);
  view.el.find(".sp-menu-row").find((r) => r.getAttribute("data-list") === "L2").fire("click");
  assert.deepEqual(rows(), ["soon"]);
  assert.equal(view.el.querySelector(".sp-picker").textContent, "Maison - 1");
  assert.equal(view.el.find(".sp-menu-row").length, 0);
});
