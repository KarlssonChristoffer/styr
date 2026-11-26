// Words for the connections an engine can reach (`connections.rs`), shared by Settings and the
// thread panel.
import type { ConnectionSource } from "./bindings/ConnectionSource";
import type { ConnectionState } from "./bindings/ConnectionState";

export const SOURCE: Record<ConnectionSource, string> = {
  claude_ai: "claude.ai account",
  user: "Your settings",
  project: "Project",
  local: "This folder",
  plugin: "Plugin",
  hub: "Hub",
  seen: "Seen in threads",
};

export const STATE: Record<ConnectionState, { label: string; tone: "ok" | "warn" | "bad" | "" }> = {
  connected: { label: "Connected", tone: "ok" },
  needs_login: { label: "Needs login", tone: "warn" },
  failed: { label: "Failed to connect", tone: "bad" },
  off: { label: "Off", tone: "" },
  unknown: { label: "Not checked", tone: "" },
};

