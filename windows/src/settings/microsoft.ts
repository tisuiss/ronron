// Settings → Integrations → Microsoft 365: one sign-in for the To Do, Calendar and
// Mail pills.
//
// The usual way is one click: Microsoft's page opens in the browser and comes
// back to Ronron by itself (authorization code + PKCE). "Use a code instead"
// falls back to the device code flow, for a browser on another device. Builds
// made with RONRON_TODO_CLIENT_ID carry their own Entra app; anyone can still
// use their own under "Use your own Entra app".

import { MUSIC_ID } from "../core/media";
import { REMINDERS_ID } from "../core/reminders";
import { Bridge, onEvent } from "../core/bridge";
import { h, clear } from "../views/dom";
import { statusDot } from "./parts";
import { t } from "../i18n/i18n";

type AuthEvent = { ok: true; account: string } | { ok: false; error: string };

/** The block on screen now: the `ms-auth` event goes to it alone. */
let current: ((e: AuthEvent) => void) | null = null;
let listening = false;

function listen() {
  if (listening) return;
  listening = true;
  void onEvent<AuthEvent>("ms-auth", (e) => current?.(e));
}

/**
 * The user's own app registration, behind a button: by default Ronron signs in with
 * the build's app (Microsoft Graph Command Line Tools unless the build says
 * otherwise), any account, no domain asked. The tenant only counts with an own app.
 */
function ownApp(): HTMLElement {
  const box = h("div", { class: "todo-own-app", style: "display:none;flex-direction:column;gap:6px;margin-top:4px" });
  const toggle = h("button", { class: "link-btn hint", style: "align-self:flex-start;padding:0", text: t("Use my own app registration…") });
  toggle.addEventListener("click", () => {
    const open = box.style.display === "none";
    box.style.display = open ? "flex" : "none";
    toggle.textContent = open ? t("Hide") : t("Use my own app registration…");
  });
  const fields: { key: string; label: string; placeholder: string }[] = [
    { key: "msgraph-client-id", label: t("Client ID"), placeholder: "00000000-0000-0000-0000-000000000000" },
    { key: "msgraph-tenant", label: t("Tenant"), placeholder: "common" },
  ];
  for (const field of fields) {
    const input = h("input", { type: "text", placeholder: field.placeholder, autocomplete: "off", spellcheck: "false", style: "flex:1 1 auto;min-width:0" }) as HTMLInputElement;
    const dot = statusDot(false);
    void Bridge.secretPresent(field.key).then((present) => {
      if (present) {
        input.placeholder = t("Stored");
        dot.style.background = "#22c55e";
        // An own app is in use: show it.
        if (field.key === "msgraph-client-id") {
          box.style.display = "flex";
          toggle.textContent = t("Hide");
        }
      }
    });
    const save = h("button", { text: t("Save") });
    save.addEventListener("click", async () => {
      const value = input.value.trim();
      try {
        await Bridge.secretSet(field.key, value);
        input.value = "";
        input.placeholder = value ? t("Stored") : field.placeholder;
        dot.style.background = value ? "#22c55e" : "#f4505e";
      } catch {
        dot.style.background = "#f5a524";
      }
    });
    box.append(h("div", { class: "row" },
      h("label", { style: "min-width:104px", text: field.label }), input, save, dot));
  }
  box.append(h("div", { class: "hint", text: t("Register an app in Microsoft Entra: a Mobile and desktop platform with the redirect URI http://localhost, public client flows allowed, delegated permissions Tasks.ReadWrite, Calendars.Read, Mail.ReadWrite and User.Read. Paste its client ID; tenant ID or domain, or leave empty for any account. Save an empty client ID to go back to the default app.") }));
  return h("div", { style: "display:flex;flex-direction:column;gap:2px" },
    h("div", { class: "hint", text: t("By default the sign-in goes through Microsoft's own app « Microsoft Graph Command Line Tools », for any account. If your organisation blocks it, use your own app registration.") }),
    toggle, box);
}

export interface HeaderPillsAccess {
  get(): string[];
  /** Saves the list; the same pills leave the declared ones (they take no slot). */
  set(ids: string[]): void;
}

const headerChoices = (): { id: string; name: string }[] => [
  { id: "integration_calendar", name: t("Calendar") },
  { id: "integration_mail", name: t("Mail") },
  { id: "integration_todo", name: "To Do" },
];

/** "In the header": each Microsoft 365 pill as a chip at the top of the island. */
function headerSwitches(access: HeaderPillsAccess): HTMLElement {
  const row = h("div", { class: "row", style: "flex-wrap:wrap;gap:14px;margin-top:4px" },
    h("label", { style: "min-width:104px", text: t("In the header") }));
  for (const choice of headerChoices()) {
    const sw = h("button", { class: access.get().includes(choice.id) ? "switch on" : "switch" });
    sw.addEventListener("click", () => {
      const on = access.get().includes(choice.id);
      access.set(on ? access.get().filter((x) => x !== choice.id) : [...access.get(), choice.id]);
      sw.classList.toggle("on", !on);
    });
    row.append(h("span", { style: "display:inline-flex;align-items:center;gap:6px;font-size:12.5px" }, sw, choice.name));
  }
  return h("div", {}, row,
    h("div", { class: "hint", text: t("Shown at the top of the island, next to the tabs, without taking a pill slot.") }));
}

export function microsoftSection(header?: HeaderPillsAccess): HTMLElement {
  listen();
  const root = h("div", { class: "todo-signin", style: "display:flex;flex-direction:column;gap:6px;padding-top:2px" });
  const status = h("div", { style: "display:flex;flex-direction:column;gap:6px" });

  function signedIn(account: string | null) {
    clear(status);
    status.append(h("div", { class: "row" },
      h("span", { style: "flex:1 1 auto;font-size:12px;color:#22c55e", text: account ? t("Signed in as {account}", { account }) : t("Connected") }),
      h("button", { text: t("Sign out"), onclick: async () => { await Bridge.msSignOut(); signedOut(null); } })));
  }

  function signedOut(error: string | null) {
    clear(status);
    const button = h("button", { class: "primary", text: t("Sign in with Microsoft") }) as HTMLButtonElement;
    button.addEventListener("click", () => {
      void Bridge.msSignInBrowser();
      inBrowser();
    });
    status.append(h("div", { class: "row" },
      h("span", { style: "flex:1 1 auto;font-size:12px;color:#f4505e", text: t("Not signed in") }), button));
    if (error) status.append(h("div", { class: "hint", style: "color:#f5a524", text: error }));
  }

  function inBrowser() {
    clear(status);
    status.append(
      h("div", { class: "row" },
        h("span", { style: "flex:1 1 auto;font-size:12px", text: t("Finish signing in in your browser…") }),
        h("button", { text: t("Cancel"), onclick: () => { void Bridge.msSignInCancel(); signedOut(null); } })),
      h("button", { class: "link-btn hint", style: "align-self:flex-start;padding:0", text: t("Use a code instead"), onclick: async () => {
        try {
          const code = await Bridge.msSignInCode();
          withCode(code.userCode, code.verificationUri);
        } catch (e) {
          signedOut(e instanceof Error ? e.message : String(e));
        }
      } }),
    );
  }

  function withCode(code: string, url: string) {
    clear(status);
    const copy = h("button", { text: t("Copy") }) as HTMLButtonElement;
    const copied = () => (copy.textContent = t("Copied"));
    copy.addEventListener("click", () => void navigator.clipboard?.writeText(code).then(copied));
    status.append(
      h("div", { class: "hint", text: t("Go to {url} and enter the code {code}.", { url, code }) }),
      h("div", { class: "row" },
        h("code", { style: "font:600 15px ui-monospace,monospace;letter-spacing:2px;flex:1 1 auto", text: code }),
        copy,
        h("button", { class: "primary", text: t("Open {name}", { name: "Microsoft" }), onclick: () => void Bridge.openUrl(url) }),
        h("button", { text: t("Cancel"), onclick: () => { void Bridge.msSignInCancel(); signedOut(null); } })),
      h("div", { class: "hint", text: t("Waiting for you to sign in…") }),
    );
    void navigator.clipboard?.writeText(code).then(copied);
  }

  current = (e) => {
    if (!root.isConnected) return;
    if (e.ok) signedIn(e.account || null);
    else signedOut(e.error);
  };

  root.append(status);
  const section = h("section", {},
    h("h2", {}, h("span", { text: "Microsoft 365" })),
    h("div", { class: "hint", text: t("One sign-in for the To Do, Calendar and Mail pills, through Microsoft Graph.") }),
    root);
  if (header) section.append(headerSwitches(header));
  void Bridge.msAccount().then((account) => {
    if (account !== null) signedIn(account);
    else signedOut(null);
    root.append(ownApp());
  });
  return section;
}

/** Music: a tab in the island's header for what plays on this computer (src/views/music.ts). */
export function musicSection(header: HeaderPillsAccess): HTMLElement {
  const sw = h("button", { class: header.get().includes(MUSIC_ID) ? "switch on" : "switch" });
  sw.addEventListener("click", () => {
    const on = header.get().includes(MUSIC_ID);
    header.set(on ? header.get().filter((x) => x !== MUSIC_ID) : [...header.get(), MUSIC_ID]);
    sw.classList.toggle("on", !on);
  });
  return h("section", {},
    h("h2", {}, h("span", { text: t("Music") })),
    h("div", { class: "hint", text: t("Play, pause and skip what plays on this computer — Spotify, YouTube Music, Amazon Music — through the system's media controls. No account to connect.") }),
    h("div", { class: "row", style: "gap:10px;margin-top:4px" }, sw, h("span", { style: "font-size:12.5px", text: t("Music tab in the header") })));
}

/** Reminders: a tab in the island's header for reminders kept on this computer (src/views/reminders.ts). */
export function remindersSection(header: HeaderPillsAccess): HTMLElement {
  const sw = h("button", { class: header.get().includes(REMINDERS_ID) ? "switch on" : "switch" });
  sw.addEventListener("click", () => {
    const on = header.get().includes(REMINDERS_ID);
    header.set(on ? header.get().filter((x) => x !== REMINDERS_ID) : [...header.get(), REMINDERS_ID]);
    sw.classList.toggle("on", !on);
  });
  return h("section", {},
    h("h2", {}, h("span", { text: t("Reminders") })),
    h("div", { class: "hint", text: t("Reminders kept on this computer, no account needed: the island opens on each one at its time, with Done and a snooze. « OK Ronron, remind me to… » adds one by voice.") }),
    h("div", { class: "row", style: "gap:10px;margin-top:4px" }, sw, h("span", { style: "font-size:12.5px", text: t("Reminders tab in the header") })));
}
