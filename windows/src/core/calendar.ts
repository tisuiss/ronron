export interface Booking {
  id: string; title: string; start: Date; end: Date;
  attendeeName: string; attendeeEmail: string; attendeeNotes: string;
  /** Microsoft 365: the Teams link, the event in Outlook on the web, and all-day events. */
  joinUrl?: string; webUrl?: string; allDay?: boolean;
  /** Which calendar it comes from (calendar365.rs: a Graph id or "ical-N"). */
  calendarId?: string;
}

/** A calendar as calendar365.rs lists it in the data. */
export interface CalendarInfo {
  id: string;
  name: string;
  color: string;
  kind: "microsoft" | "ical";
  shown: boolean;
  /** Minutes before an event; 0 = no reminder. */
  remind: number;
}

export function readCalendars(value: unknown): CalendarInfo[] {
  if (!Array.isArray(value)) return [];
  return value.filter((c) => c && typeof c.id === "string").map((c) => ({
    id: c.id,
    name: typeof c.name === "string" ? c.name : "",
    color: typeof c.color === "string" && /^#[0-9a-fA-F]{6}$/.test(c.color) ? c.color : "#0F78D4",
    kind: c.kind === "ical" ? "ical" : "microsoft",
    shown: c.shown !== false,
    remind: typeof c.remind === "number" && c.remind > 0 ? c.remind : 0,
  }));
}

/** Invalid data must not appear as an empty, successfully loaded calendar. */
export function readBookings(value: unknown): Booking[] | null {
  if (!Array.isArray(value)) return null;
  const out: Booking[] = [];
  for (const raw of value) {
    if (!raw || typeof raw.id !== "string" || typeof raw.start !== "string" || typeof raw.end !== "string") return null;
    const start = new Date(raw.start), end = new Date(raw.end);
    if (!Number.isFinite(start.getTime()) || !Number.isFinite(end.getTime()) || end < start) return null;
    const text = (key: string) => typeof raw[key] === "string" ? raw[key] : "";
    const https = (key: string) => (text(key).startsWith("https://") ? text(key) : undefined);
    const booking: Booking = { id: raw.id, title: text("title"), start, end, attendeeName: text("attendeeName"),
      attendeeEmail: text("attendeeEmail"), attendeeNotes: text("attendeeNotes") };
    if (https("joinUrl")) booking.joinUrl = https("joinUrl");
    if (https("webUrl")) booking.webUrl = https("webUrl");
    if (raw.allDay === true) booking.allDay = true;
    if (typeof raw.calendarId === "string") booking.calendarId = raw.calendarId;
    out.push(booking);
  }
  return out.sort((a, b) => a.start.getTime() - b.start.getTime());
}

/** Local calendar dates, independent of UTC offset and daylight-saving transitions. */
export function dayKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}
export function monthIndex(date: Date): number { return date.getFullYear() * 12 + date.getMonth(); }
export function monthDays(month: Date): Date[] {
  const first = new Date(month.getFullYear(), month.getMonth(), 1);
  const offset = (first.getDay() + 6) % 7;
  return Array.from({ length: 42 }, (_, index) => new Date(first.getFullYear(), first.getMonth(), index + 1 - offset));
}
