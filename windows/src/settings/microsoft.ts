// Settings → Integrations → Microsoft 365: one sign-in for the To Do, Calendar and
// Mail pills.
//
// The usual way is one click: Microsoft's page opens in the browser and comes
// back to Ronron by itself (authorization code + PKCE). "Use a code instead"
// falls back to the device code flow, for a browser on another device. Builds
// made with RONRON_TODO_CLIENT_ID carry their own Entra app; anyone can still
// use their own under "Use your own Entra app".

import { MUSIC_ID } from "../core/media";
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

/** The client ID and tenant of the user's own app, folded away when the build has one. */
function ownApp(builtin: boolean): HTMLElement {
  const details = h("details", { class: "todo-own-app" }) as HTMLDetailsElement;
  details.open = !builtin;
  details.append(h("summary", { class: "hint", style: "cursor:pointer", text: t("Use your own Entra app") }));
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
    details.append(h("div", { class: "row", style: "margin-top:6px" },
      h("label", { style: "min-width:104px", text: field.label }), input, save, dot));
  }
  details.append(h("div", { class: "hint", text: t("Register an app in Microsoft Entra: a Mobile and desktop platform with the redirect URI http://localhost, public client flows allowed, delegated permissions Tasks.ReadWrite, Calendars.Read, Mail.ReadWrite and User.Read. Paste its client ID; tenant ID or domain, or leave empty for any account.") }));
  return details;
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
  let builtin = false;

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
  void Promise.all([Bridge.msBuiltinApp(), Bridge.msAccount()]).then(([hasApp, account]) => {
    builtin = hasApp === true;
    if (account !== null) signedIn(account);
    else signedOut(null);
    root.append(ownApp(builtin));
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
