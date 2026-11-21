// Settings → Integrations → Google: one sign-in for Gmail (the Mail tab), Google
// Calendar (the Calendar tab) and Google Tasks (the To Do tab), beside Microsoft 365
// or instead of it.
//
// Each user signs in with their own Google Cloud project: its OAuth client (type
// "Desktop app") goes to the credential store, then Google's page opens in the
// browser and comes back to Ronron by itself (src-tauri/src/google/auth.rs).

import { Bridge, onEvent } from "../core/bridge";
import { h, clear } from "../views/dom";
import { statusDot } from "./parts";
import { t } from "../i18n/i18n";

type AuthEvent = { ok: true; account: string } | { ok: false; error: string };

const CONSOLE_URL = "https://console.cloud.google.com/apis/credentials";

/** The block on screen now: the `google-auth` event goes to it alone. */
let current: ((e: AuthEvent) => void) | null = null;
let listening = false;

function listen() {
  if (listening) return;
  listening = true;
  void onEvent<AuthEvent>("google-auth", (e) => current?.(e));
}

/** The client ID and secret of the user's Google Cloud project; `ready` says both are stored. */
function clientFields(onChange: () => void): { el: HTMLElement; ready: () => Promise<boolean> } {
  const el = h("div", { style: "display:flex;flex-direction:column;gap:6px" });
  const fields: { key: string; label: string; placeholder: string; secret: boolean }[] = [
    { key: "google-client-id", label: t("Client ID"), placeholder: "000000000000-xxxx.apps.googleusercontent.com", secret: false },
    { key: "google-client-secret", label: t("Client secret"), placeholder: "GOCSPX-…", secret: true },
  ];
  for (const field of fields) {
    const input = h("input", {
      type: field.secret ? "password" : "text", placeholder: field.placeholder,
      autocomplete: "off", spellcheck: "false", style: "flex:1 1 auto;min-width:0",
    }) as HTMLInputElement;
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
        onChange();
      } catch {
        dot.style.background = "#f5a524";
      }
    });
    el.append(h("div", { class: "row" }, h("label", { style: "min-width:104px", text: field.label }), input, save, dot));
  }
  el.append(
    h("div", { class: "hint", text: t("In Google Cloud: create a project, turn on the Gmail, Google Calendar and Google Tasks APIs, set up the consent screen (External, then Publish app so the sign-in lasts) and create an OAuth client of type Desktop app. Paste its client ID and secret here.") }),
    h("button", { class: "link-btn hint", style: "align-self:flex-start;padding:0", text: t("Open {name}", { name: "Google Cloud" }), onclick: () => void Bridge.openUrl(CONSOLE_URL) }),
  );
  const ready = async () =>
    ((await Bridge.secretPresent("google-client-id")) ?? false) && ((await Bridge.secretPresent("google-client-secret")) ?? false);
  return { el, ready };
}

export function googleSection(): HTMLElement {
  listen();
  const status = h("div", { style: "display:flex;flex-direction:column;gap:6px" });
  const client = clientFields(() => void redraw());
  const details = h("details", { class: "todo-own-app" }) as HTMLDetailsElement;
  details.append(h("summary", { class: "hint", style: "cursor:pointer", text: t("Your Google Cloud project") }), client.el);

  function signedIn(account: string | null) {
    clear(status);
    details.open = false;
    status.append(h("div", { class: "row" },
      h("span", { style: "flex:1 1 auto;font-size:12px;color:#22c55e", text: account ? t("Signed in as {account}", { account }) : t("Connected") }),
      h("button", { text: t("Sign out"), onclick: async () => { await Bridge.googleSignOut(); void redraw(); } })));
  }

  async function signedOut(error: string | null) {
    clear(status);
    const ready = await client.ready();
    details.open = !ready;
    const button = h("button", { class: "primary", text: t("Sign in with Google") }) as HTMLButtonElement;
    button.disabled = !ready;
    button.addEventListener("click", () => {
      void Bridge.googleSignIn();
      inBrowser();
    });
    status.append(h("div", { class: "row" },
      h("span", { style: "flex:1 1 auto;font-size:12px;color:#f4505e", text: t("Not signed in") }), button));
    if (error) status.append(h("div", { class: "hint", style: "color:#f5a524", text: error }));
  }

  function inBrowser() {
    clear(status);
    status.append(h("div", { class: "row" },
      h("span", { style: "flex:1 1 auto;font-size:12px", text: t("Finish signing in in your browser…") }),
      h("button", { text: t("Cancel"), onclick: () => { void Bridge.googleSignInCancel(); void signedOut(null); } })));
  }

  async function redraw() {
    const account = await Bridge.googleAccount();
    if (account !== null && account !== undefined) signedIn(account);
    else await signedOut(null);
  }

  const root = h("div", { style: "display:flex;flex-direction:column;gap:6px;padding-top:2px" }, status, details);
  current = (e) => {
    if (!root.isConnected) return;
    if (e.ok) signedIn(e.account || null);
    else void signedOut(e.error);
  };
  void redraw();
  return h("section", {},
    h("h2", {}, h("span", { text: "Google" })),
    h("div", { class: "hint", text: t("One sign-in for Gmail, Google Calendar and Google Tasks, beside Microsoft 365 or instead of it.") }),
    root);
}
