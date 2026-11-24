// The notifications of this run (src/core/notifications.ts): the history behind
// the header's bell, and what waits behind the view on screen.

import { test } from "node:test";
import assert from "node:assert/strict";
import { MAX_NOTICES, NoticeLog, WAITING_VIEWS, waitingViews } from "../src/core/notifications.ts";

test("newest first, counted unread until read, capped", () => {
  const log = new NoticeLog();
  let changes = 0;
  log.onChange(() => changes++);
  log.push({ kind: "reminder", title: "Point client", detail: "14:00", at: 1_000 });
  log.push({ kind: "request", title: "Claude Code", detail: "Bash · npm test", ref: "req-1", at: 2_000 });
  assert.deepEqual(log.items.map((n) => n.title), ["Claude Code", "Point client"]);
  assert.equal(log.unread(), 2);
  assert.equal(changes, 2);
  log.markAllRead();
  assert.equal(log.unread(), 0);
  for (let i = 0; i < MAX_NOTICES + 5; i++) log.push({ kind: "message", title: "WhatsApp", detail: `Anne ${i}`, at: 10_000 + i });
  assert.equal(log.items.length, MAX_NOTICES);
  log.clear();
  assert.equal(log.items.length, 0);
});

test("the same news twice within a minute is one notice", () => {
  const log = new NoticeLog();
  assert.ok(log.push({ kind: "finished", title: "Codex", detail: "Done", at: 0 }));
  assert.equal(log.push({ kind: "finished", title: "Codex", detail: "Done", at: 30_000 }), null);
  assert.ok(log.push({ kind: "finished", title: "Codex", detail: "Done", at: 90_000 }));
});

test("a request answered or a reminder dealt with is read", () => {
  const log = new NoticeLog();
  log.push({ kind: "request", title: "Claude Code", detail: "Bash", ref: "req-1" });
  log.push({ kind: "reminder", title: "Standup", detail: "09:30", ref: "reminder:x@1" });
  log.settle("req-1");
  assert.equal(log.unread(), 1);
  assert.equal(log.items.find((n) => n.ref === "req-1").read, true);
});

test("what waits: a request first, then the calendar reminder, then your reminder", () => {
  assert.deepEqual(waitingViews({ pendingApproval: null, reminder: null, alarm: null }), []);
  assert.deepEqual(waitingViews({ pendingApproval: {}, reminder: {}, alarm: {} }), ["approval", "reminder", "alarm"]);
  assert.deepEqual(waitingViews({ pendingApproval: { questions: [] }, reminder: {}, alarm: null }), ["question", "reminder"]);
  assert.ok(WAITING_VIEWS.has("reminder") && !WAITING_VIEWS.has("finished"));
});
