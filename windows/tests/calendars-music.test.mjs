// 0.8.0: several calendars and their reminders, the overview's four slots, the Music tab.

import test from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { sent, internals } from "./tauri.mjs";
import { State } from "../src/core/state.ts";
import { readCalendars } from "../src/core/calendar.ts";
import { dueReminders } from "../src/island/reminders.ts";
import { freeSlot, prefFor, setPref, remindLabel } from "../src/settings/calendars.ts";
import { headerPillIds } from "../src/core/pills.ts";
import { clock } from "../src/core/media.ts";

installFakeDom();

const answers = {};
const plainInvoke = internals.invoke;
internals.invoke = async (cmd, args) => {
  const result = await plainInvoke(cmd, args);
  return cmd in answers ? answers[cmd] : result;
};

const iso = (ms) => new Date(ms).toISOString();

test("calendars are read with their colour, shown and reminder", () => {
  const list = readCalendars([
    { id: "A", name: "Calendrier", color: "#0F78D4", kind: "microsoft", shown: true, remind: 10 },
    { id: "ical-0", name: "Famille", color: "nope", kind: "ical", shown: false, remind: -3 },
    { name: "no id" },
  ]);
  assert.equal(list.length, 2);
  assert.deepEqual([list[1].kind, list[1].shown, list[1].remind, list[1].color], ["ical", false, 0, "#0F78D4"]);
});

test("a reminder comes only for shown calendars that remind, before the start, once", () => {
  const now = Date.now();
  const data = {
    calendars: [
      { id: "A", name: "Travail", color: "#0F78D4", kind: "microsoft", shown: true, remind: 10 },
      { id: "B", name: "Perso", color: "#E879A9", kind: "ical", shown: true, remind: 0 },
      { id: "C", name: "Caché", color: "#34D399", kind: "ical", shown: false, remind: 15 },
    ],
    bookings: [
      { id: "soon", calendarId: "A", title: "Point client", start: iso(now + 5 * 60_000), end: iso(now + 35 * 60_000) },
      { id: "later", calendarId: "A", title: "Revue", start: iso(now + 3 * 3_600_000), end: iso(now + 4 * 3_600_000) },
      { id: "quiet", calendarId: "B", title: "Sport", start: iso(now + 600_000), end: iso(now + 900_000) },
      { id: "hidden", calendarId: "C", title: "X", start: iso(now + 600_000), end: iso(now + 900_000) },
      { id: "past", calendarId: "A", title: "Old", start: iso(now - 600_000), end: iso(now + 600_000) },
      { id: "day", calendarId: "A", title: "Férié", start: iso(now + 3_600_000), end: iso(now + 86_400_000), allDay: true },
    ],
  };
  const due = dueReminders(data, now, new Set());
  assert.deepEqual(due.map((d) => d.booking.id), ["soon", "later"]);
  assert.equal(due[0].at, Date.parse(data.bookings[0].start) - 10 * 60_000);
  assert.deepEqual(dueReminders(data, now, new Set([due[0].key])).map((d) => d.booking.id), ["later"]);
});

test("Settings keep one choice per calendar and up to eight iCal calendars", () => {
  const settings = { calendarPrefs: [] };
  assert.deepEqual(prefFor(settings.calendarPrefs, "A", true), { id: "A", shown: true, remind: 0 });
  setPref(settings, { id: "A", shown: true, remind: 15 });
  setPref(settings, { id: "A", shown: false, remind: 15 });
  assert.deepEqual(settings.calendarPrefs, [{ id: "A", shown: false, remind: 15 }]);
  assert.equal(freeSlot([{ slot: 0 }, { slot: 2 }]), 1);
  assert.equal(freeSlot(Array.from({ length: 8 }, (_, slot) => ({ slot }))), null);
  assert.equal(remindLabel(10), "10 min before");
  assert.equal(remindLabel(0), "No reminder");
});

test("the Music tab sits last in the header and is not a pill", async () => {
  const { headerTabs } = await import("../src/views/views.ts");
  assert.deepEqual(headerPillIds(["music", "integration_todo"]), ["integration_todo", "music"]);
  State.settings.headerPills = ["music"];
  assert.deepEqual(headerTabs(Date.now()).map((t) => [t.view, t.label]), [["music", "Music"]]);
  State.loadIntegrationTasks();
  assert.ok(!State.tasks.some((t) => t.id === "music"));
  State.settings.headerPills = [];
  assert.equal(clock(187), "3:07");
  assert.equal(clock(3765), "1:02:45");
});

test("the Music space shows the track and drives the player", async () => {
  const { buildMusicSpace } = await import("../src/views/music.ts");
  answers.media_state = { sources: [{ id: "Spotify.exe", name: "Spotify", playing: true }, { id: "MSEdge", name: "Edge", playing: false }],
    now: { source: "Spotify.exe", title: "Bohemian Rhapsody", artist: "Queen", album: "A Night at the Opera", playing: true, position: 60, duration: 354, artKey: "k1", canPrevious: true, canNext: true } };
  answers.media_art = null;
  State.settings.headerPills = ["music"];
  State.view = "music";
  State.mode = "expanded";
  const did = [];
  const view = buildMusicSpace({ blip() {}, setView: (v) => did.push(v) });
  view.sync();
  await new Promise((r) => setTimeout(r, 20));
  assert.match(view.el.textContent, /Bohemian Rhapsody/);
  assert.equal(view.el.find(".sp-chip").length, 2);
  view.el.querySelector(".play").fire("click");
  await new Promise((r) => setTimeout(r, 5));
  assert.deepEqual(sent("media_control").at(-1), { source: "Spotify.exe", action: "playPause" });
  view.el.find(".mu-btn").at(-1).fire("click");
  await new Promise((r) => setTimeout(r, 5));
  assert.deepEqual(sent("media_control").at(-1), { source: "Spotify.exe", action: "next" });
  State.view = "overview";
  State.settings.headerPills = [];
  await new Promise((r) => setTimeout(r, 1600));
});

test("a pill is busy at work, with a badge, or with unread WhatsApp; asleep otherwise", async () => {
  const { pillBusy } = await import("../src/core/state.ts");
  const pill = (over) => ({ id: "agent_claude", state: "idle", pillBadge: null, ...over });
  assert.equal(pillBusy(pill({ state: "working" })), true);
  assert.equal(pillBusy(pill({ state: "approval" })), true);
  assert.equal(pillBusy(pill({ pillBadge: "finished" })), true);
  assert.equal(pillBusy(pill({})), false);
  assert.equal(pillBusy(pill({ state: "finished" })), false);
  assert.equal(pillBusy(pill({ id: "integration_whatsapp" }), 3), true);
  assert.equal(pillBusy(pill({ id: "integration_whatsapp" }), 0), false);
  assert.equal(pillBusy(pill({ id: "integration_github" })), false);
});
