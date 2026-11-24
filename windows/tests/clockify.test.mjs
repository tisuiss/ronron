// Clockify in the island (src/core/clockify.ts): the hours, the entries to
// start again, and when the island offers to start or stop a timer.

import { test } from "node:test";
import assert from "node:assert/strict";
import { folderName, hm, hms, readClockify, recent, suggestion, totals } from "../src/core/clockify.ts";

// Wednesday 25 November 2026, 10:00 local.
const NOW = new Date(2026, 10, 25, 10, 0).getTime();
const H = 3600_000;
const entry = (id, start, end, description = "Code", projectId = "p1", projectName = "Ronron") =>
  ({ id, description, projectId, projectName, projectColor: "#E91E63", start, end });
const data = (entries, running = null) => ({ running, entries, projects: [{ id: "p1", name: "Ronron", color: "#E91E63", client: "" }] });

test("hours read as h:mm, the running timer with its seconds", () => {
  assert.equal(hm(65 * 60_000), "1:05");
  assert.equal(hm(-5), "0:00");
  assert.equal(hms(H + 2 * 60_000 + 9_000), "1:02:09");
});

test("today and this week, from Monday, the running timer counted to now", () => {
  const monday = new Date(2026, 10, 23, 9, 0).getTime();
  const lastSunday = new Date(2026, 10, 22, 22, 0).getTime();
  const d = data([
    entry("run", NOW - H, null),
    entry("a", NOW - 3 * H, NOW - 2 * H),
    entry("b", monday, monday + 2 * H),
    entry("c", lastSunday, lastSunday + 4 * H), // 2 h of it on Monday
  ]);
  const sum = totals(d, NOW);
  assert.equal(sum.today, 2 * H);
  assert.equal(sum.week, (1 + 1 + 2 + 2) * H);
});

test("the last different things worked on, the running one left out", () => {
  const d = data([
    entry("run", NOW - H, null, "Now"),
    entry("a", NOW - 3 * H, NOW - 2 * H, "Code"),
    entry("b", NOW - 5 * H, NOW - 4 * H, "code"),
    entry("c", NOW - 7 * H, NOW - 6 * H, "Mails", "p2", "Admin"),
    entry("d", NOW - 9 * H, NOW - 8 * H, "", null, ""),
  ]);
  assert.deepEqual(recent(d, 3).map((e) => e.id), ["a", "c"]);
});

test("what the Rust side sends is read, anything else is not", () => {
  assert.equal(readClockify(null), null);
  assert.equal(readClockify({}), null);
  assert.equal(readClockify({ entries: [], running: null }).projects.length, 0);
  assert.equal(folderName("C:\\code\\ronron\\"), "ronron");
  assert.equal(folderName("/home/me/site"), "site");
});

const base = (over = {}) => ({
  now: NOW, data: data([]), agents: [], events: [], folders: {}, dismissed: new Set(), ...over,
});

test("an agent at work with no timer: start on the folder's project", () => {
  const agents = [{ name: "Claude Code", folder: "C:\\code\\ronron", busy: true }];
  const s = suggestion(base({ agents, folders: { "C:\\code\\ronron": "p1" } }));
  assert.equal(s.kind, "agent");
  assert.equal(s.projectId, "p1");
  assert.equal(s.description, "ronron");
  // A project that is gone is not offered; an idle agent offers nothing.
  assert.equal(suggestion(base({ agents, folders: { "C:\\code\\ronron": "gone" } })).projectId, null);
  assert.equal(suggestion(base({ agents: [{ ...agents[0], busy: false }] })), null);
  // Waved away: not again today for that folder.
  assert.equal(suggestion(base({ agents, dismissed: new Set([s.key]) })), null);
});

test("a meeting starting offers a timer on its title, from 2 minutes before to 15 in", () => {
  const ev = (start) => ({ id: "m1", title: "Point client", start: new Date(start).toISOString(), end: new Date(start + H).toISOString() });
  assert.equal(suggestion(base({ events: [ev(NOW + 60_000)] })).title, "Point client");
  assert.equal(suggestion(base({ events: [ev(NOW - 10 * 60_000)] })).kind, "meeting");
  assert.equal(suggestion(base({ events: [ev(NOW + 5 * 60_000)] })), null);
  assert.equal(suggestion(base({ events: [ev(NOW - 20 * 60_000)] })), null);
});

test("with a timer running, only a very long or late one is pointed out", () => {
  const agents = [{ name: "Codex", folder: "/w", busy: true }];
  assert.equal(suggestion(base({ agents, data: data([], entry("r", NOW - H, null)) })), null);
  assert.equal(suggestion(base({ data: data([], entry("r", NOW - 9 * H, null)) })).kind, "long");
  const late = new Date(2026, 10, 25, 21, 30).getTime();
  assert.equal(suggestion(base({ now: late, data: data([], entry("r", late - H, null)) })).kind, "long");
  assert.equal(suggestion(base({ data: null, agents })), null);
});

test("an export covers whole local weeks from Monday and whole months from the 1st", async () => {
  const { exportRange } = await import("../src/core/clockify.ts");
  const day = (ms) => { const d = new Date(ms); return [d.getFullYear(), d.getMonth() + 1, d.getDate(), d.getHours()]; };
  // NOW is Wednesday 25 November 2026.
  let r = exportRange("thisWeek", NOW);
  assert.deepEqual([day(r.from), day(r.to)], [[2026, 11, 23, 0], [2026, 11, 30, 0]]);
  r = exportRange("lastWeek", NOW);
  assert.deepEqual([day(r.from), day(r.to)], [[2026, 11, 16, 0], [2026, 11, 23, 0]]);
  r = exportRange("thisMonth", NOW);
  assert.deepEqual([day(r.from), day(r.to)], [[2026, 11, 1, 0], [2026, 12, 1, 0]]);
  r = exportRange("lastMonth", new Date(2026, 0, 10).getTime());
  assert.deepEqual([day(r.from), day(r.to)], [[2025, 12, 1, 0], [2026, 1, 1, 0]]);
});
