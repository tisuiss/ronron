// The chat on the user's own subscription, as the island sees it: which
// providers answer through their terminal tool (cli_relay.rs), asked of Rust
// and kept until the next ask.

import { Bridge, type SubscriptionStatus } from "./bridge";
import type { Settings } from "./state";

let rows: SubscriptionStatus[] = [];
/** A remote VS Code window with the Ronron extensions is open (vscode_bridge.rs). */
let vscode = false;

/** Asks Rust again (it looks for the tools on every call). */
export async function loadSubscriptions(): Promise<SubscriptionStatus[]> {
  const [next, open] = await Promise.all([Bridge.subscriptionStatus(), Bridge.vscodeAvailable()]);
  rows = next ?? rows;
  vscode = open === true;
  return rows;
}

export function setSubscriptions(next: SubscriptionStatus[] | null | undefined) {
  if (next) rows = next;
}

export function subscription(provider: string): SubscriptionStatus | undefined {
  return rows.find((r) => r.provider === provider);
}

/** The providers whose switch is on. */
export function subscribedProviders(): Set<string> {
  const out = new Set(rows.filter((r) => r.on).map((r) => r.provider));
  if (vscode) out.add("vscode");
  return out;
}

/** The model chip: the tool's name when it answers (with Claude's model for Claude Code). */
export function chipLabel(settings: Settings, model: string): string {
  // A VS Code workspace: "<window>|/home/greg/ronron" → "ronron".
  if (settings.chatProvider === "vscode") {
    const folder = model.split("|").slice(1).join("|");
    return folder.split("/").filter(Boolean).pop() || "VS Code";
  }
  const sub = subscription(settings.chatProvider);
  if (!sub?.on) return model;
  return settings.chatProvider === "anthropic" && model ? `${model} · ${sub.tool}` : sub.tool;
}
