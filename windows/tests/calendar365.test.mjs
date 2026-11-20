// The Microsoft 365 calendar pill: Graph's events in the Cal.com card and month view.

import test from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { sent } from "./tauri.mjs";
import { State } from "../src/core/state.ts";
import { readBookings } from "../src/core/calendar.ts";
import { pillDefinition } from "../src/core/pills.ts";
import { hasIntegrationData, idleStatus, renderIntegrationCard } from "../src/views/integrations.ts";

installFakeDom();

const ID = "integration_calendar";
const iso = (ms) => new Date(ms).toISOString();

test("the pill shares the Microsoft 365 sign-in", () => {
  assert.deepEqual(pillDefinition(ID).connect, { kind: "key", key: "msgraph-refresh-0" });
  assert.equal(idleStatus(ID, { configured: false, error: null }, "").label, "Not signed in");
});

test("Teams and Outlook links are kept only when they are https", () => {
  const [a, b] = readBookings([
    { id: "1", start: "2026-11-20T09:00:00Z", end: "2026-11-20T10:00:00Z", joinUrl: "https://teams.microsoft.com/x", webUrl: "https://outlook.office365.com/y", allDay: false },
    { id: "2", start: "2026-11-21T00:00:00Z", end: "2026-11-22T00:00:00Z", joinUrl: "javascript:alert(1)", allDay: true },
  ]);
  assert.equal(a.joinUrl, "https://teams.microsoft.com/x");
  assert.equal(a.webUrl, "https://outlook.office365.com/y");
  assert.equal(b.joinUrl, undefined);
  assert.equal(b.allDay, true);
});

test("the card shows what is under way and next, past events left out", () => {
  const now = Date.now();
  State.integrations[ID] = {
    data: { rangeStart: now - 86_400_000, rangeEnd: now + 30 * 86_400_000, bookings: [
      { id: "past", title: "Hier", start: iso(now - 7_200_000), end: iso(now - 3_600_000) },
      { id: "now", title: "Point client", start: iso(now - 600_000), end: iso(now + 1_200_000), joinUrl: "https://teams.microsoft.com/x" },
      { id: "next", title: "Revue", start: iso(now + 3_600_000), end: iso(now + 5_400_000) },
    ] },
    error: null, loaded: true, configured: true,
  };
  assert.ok(hasIntegrationData(ID));
  const hooks = { detailOpen: false, openDetail() {}, closeDetail() {}, openSettings() {} };
  const card = renderIntegrationCard({ id: ID, steps: [] }, hooks);
  assert.match(card.textContent, /Calendar/);
  assert.doesNotMatch(card.textContent, /Hier/);
  assert.match(card.textContent, /Now/);
  assert.match(card.textContent, /Point client/);

  // The detail: day → event → Join the meeting.
  const detail = renderIntegrationCard({ id: ID, steps: [] }, { ...hooks, detailOpen: true });
  const day = detail.find(".cal-day").find((el) => !el.disabled && /•/.test(el.textContent));
  day.fire("click");
  detail.find(".cal-booking").find((el) => /Point client/.test(el.textContent)).fire("click");
  detail.querySelector(".cal-join").fire("click");
  assert.deepEqual(sent("open_url").at(-1), { url: "https://teams.microsoft.com/x" });
});
