// Reminders kept on this computer (src/core/reminders.ts, src/views/reminders.ts,
// src/island/alarms.ts): the list from Rust, the quick times, the tab and the ringing.

import { afterEach, beforeEach, mock, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { emit, sent } from "./tauri.mjs";
import { State } from "../src/core/state.ts";
import { setLanguage } from "../src/i18n/i18n.ts";
import {
  REMINDERS_ID, ReminderList, fromLocalInput, quickTimes, readReminders, startReminders, toLocalInput, whenLabel,
} from "../src/core/reminders.ts";
import { buildRemindersSpace } from "../src/views/reminders.ts";
import { Alarms, dueAlarm } from "../src/island/alarms.ts";
import { headerTabs } from "../src/views/views.ts";
import { HEADER_PILLS } from "../src/core/pills.ts";

installFakeDom();
startReminders();

const MIN = 60_000;

function actions() {
  const did = { views: [], hold: [] };
  return { did, blip() {}, setView: (v) => did.views.push(v), keyboard() {}, holdOpen: (on) => did.hold.push(on) };
}

beforeEach(() => {
  setLanguage("en");
  State.view = "reminders";
  State.mode = "expanded";
  State.alarm = null;
  State.pendingApproval = null;
  State.settings.headerPills = [REMINDERS_ID];
  emit("reminders", []);
});

afterEach(() => mock.timers.reset());

test("the list from Rust is read, soonest first, odd entries left out", () => {
  const list = readReminders([
    { id: "b", text: "Appeler Anne", at: 2000, created: 1 },
    { id: "a", text: "Thé", at: 1000 },
    { id: "x", text: 3, at: 5 },
    { text: "no id", at: 5 },
    null,
  ]);
  assert.deepEqual(list.map((r) => r.id), ["a", "b"]);
  assert.deepEqual(readReminders("nope"), []);
});

test("the quick times: in 10 min, in 1 h, this evening while it is ahead, tomorrow 9:00", () => {
  const morning = new Date(2026, 10, 22, 10, 0);
  const q = quickTimes(morning);
  assert.equal(q.length, 4);
  assert.equal(q[0].at - morning.getTime(), 10 * MIN);
  assert.equal(q[1].at - morning.getTime(), 60 * MIN);
  assert.equal(new Date(q[2].at).getHours(), 18);
  const tomorrow = new Date(q[3].at);
  assert.equal(tomorrow.getDate(), 23);
  assert.equal(tomorrow.getHours(), 9);
  // At 17:45, this evening is gone.
  assert.equal(quickTimes(new Date(2026, 10, 22, 17, 45)).length, 3);
});

test("when a reminder rings, in words; datetime-local round trip", () => {
  const now = new Date(2026, 10, 22, 10, 0);
  assert.match(whenLabel(new Date(2026, 10, 22, 15, 0).getTime(), now), /^Today 15:00|^Today 03:00/);
  assert.match(whenLabel(new Date(2026, 10, 23, 9, 0).getTime(), now), /^Tomorrow/);
  const at = new Date(2026, 10, 24, 18, 30).getTime();
  assert.equal(fromLocalInput(toLocalInput(at)), at);
  assert.equal(toLocalInput(at), "2026-11-24T18:30");
  assert.equal(fromLocalInput("demain"), null);
});

test("the tab: the list, a quick time, and Add send it to Rust", () => {
  const now = Date.now();
  emit("reminders", [
    { id: "r1", text: "Appeler Anne", at: now + 30 * MIN },
    { id: "r2", text: "Sortir le chien", at: now + 120 * MIN },
  ]);
  const a = actions();
  const view = buildRemindersSpace(a);
  view.sync();
  assert.deepEqual(view.el.find(".sp-task").map((r) => r.getAttribute("data-reminder")), ["r1", "r2"]);
  assert.match(view.el.textContent, /Appeler Anne/);

  // ✕ removes one.
  view.el.find(".rm-remove")[1].fire("click");
  assert.deepEqual(sent("reminders_remove").at(-1), { id: "r2" });

  // No text: nothing sent.
  const input = view.el.querySelector(".sp-add-input");
  input.value = "";
  view.el.querySelector(".rm-when").value = "";
  const before = sent("reminders_add").length;
  view.el.querySelector(".sp-add-go").fire("click");
  assert.equal(sent("reminders_add").length, before);

  // A text and "In 10 min".
  input.value = "Boire de l'eau";
  view.el.find(".sp-chip")[0].fire("click");
  view.el.querySelector(".sp-add-go").fire("click");
  const added = sent("reminders_add").at(-1);
  assert.equal(added.text, "Boire de l'eau");
  assert.ok(Math.abs(added.at - (Date.now() + 10 * MIN)) < 5_000, `${added.at}`);
});

test("the header tab counts the reminders and turns red once one has rung", () => {
  assert.ok(HEADER_PILLS.includes(REMINDERS_ID));
  const now = Date.now();
  emit("reminders", [{ id: "r1", text: "Plus tard", at: now + 60 * MIN }]);
  let tab = headerTabs(now).find((x) => x.id === REMINDERS_ID);
  assert.deepEqual([tab.view, tab.badge, tab.alert], ["reminders", "1", false]);
  emit("reminders", [{ id: "r1", text: "Déjà", at: now - MIN }]);
  tab = headerTabs(now).find((x) => x.id === REMINDERS_ID);
  assert.equal(tab.alert, true);
});

test("dueAlarm: the soonest come due, not the one on screen nor one just answered", () => {
  const list = [{ id: "a", text: "A", at: 100 }, { id: "b", text: "B", at: 200 }, { id: "c", text: "C", at: 900 }];
  assert.equal(dueAlarm(list, 500, null).id, "a");
  assert.equal(dueAlarm(list, 500, "a").id, "b");
  assert.equal(dueAlarm(list, 500, null, new Map([["a", 100]])).id, "b");
  // Snoozed (moved on): it may ring again at its new time.
  assert.equal(dueAlarm([{ id: "a", text: "A", at: 400 }], 500, null, new Map([["a", 100]])).id, "a");
  assert.equal(dueAlarm(list, 50, null), null);
});

test("a reminder rings when its time comes; Done removes it and it does not ring again", () => {
  mock.timers.enable({ apis: ["setTimeout", "Date"], now: 1_000_000 });
  const host = { alerts: [], pinned: [], closed: 0,
    alert(v) { this.alerts.push(v); }, emote() {}, setPinned(on) { this.pinned.push(on); }, closeAlarm() { this.closed++; } };
  emit("reminders", [{ id: "r1", text: "Appeler Anne", at: 1_000_000 + 5 * MIN }]);
  const alarms = new Alarms(host);
  mock.timers.tick(5 * MIN - 1000);
  assert.deepEqual(host.alerts, []);
  mock.timers.tick(1000);
  assert.deepEqual(host.alerts, ["alarm"]);
  assert.equal(State.alarm?.id, "r1");

  alarms.done();
  assert.equal(State.alarm, null);
  assert.deepEqual(sent("reminders_remove").at(-1), { id: "r1" });
  // Rust has not answered yet: the same reminder is still listed, but stays quiet.
  State.notify();
  mock.timers.tick(10_000);
  assert.deepEqual(host.alerts, ["alarm"]);
  emit("reminders", []);
});

test("a snooze asks Rust to move it on", () => {
  mock.timers.enable({ apis: ["setTimeout", "Date"], now: 5_000_000 });
  const host = { alerts: [], alert(v) { this.alerts.push(v); }, emote() {}, setPinned() {}, closeAlarm() {} };
  emit("reminders", [{ id: "r9", text: "Thé", at: 4_000_000 }]);
  const alarms = new Alarms(host);
  mock.timers.tick(10);
  assert.equal(State.alarm?.id, "r9", "a reminder already due rings at once");
  alarms.snooze(10);
  assert.deepEqual(sent("reminders_snooze").at(-1), { id: "r9", minutes: 10 });
  assert.equal(State.alarm, null);
  emit("reminders", []);
});

test("ReminderList follows the reminders event", () => {
  emit("reminders", [{ id: "z", text: "Z", at: 1 }]);
  assert.equal(ReminderList.items[0].id, "z");
});
