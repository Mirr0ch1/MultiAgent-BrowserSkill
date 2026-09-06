//! M3: runtime-configurable gateway endpoint.
//!
//! The extension used to hard-code `ws://127.0.0.1:52800` at build time
//! (`__BSK_DAEMON_WS_URL__` in wxt.config.ts). Gateway mode needs the
//! popup to point the extension at a LAN/Tailscale daemon instead, and
//! (when the gateway enforces an extension token) to present that token
//! during the handshake.
//!
//! All values live in `chrome.storage.local`; unset falls back to the
//! build-time default so single-machine users never have to touch the
//! popup.

import type { StorageBackend } from "./instance-id";
import { defaultStorage } from "./instance-id";

const DAEMON_WS_URL_KEY = "bsk_daemon_ws_url";
const EXTENSION_TOKEN_KEY = "bsk_extension_token";

/**
 * Build-time default WebSocket endpoint (`wxt.config.ts` define, default
 * `ws://127.0.0.1:52800`). Imported dynamically via the global so the
 * value exists in both Vite (extension) and Vitest (which keeps the
 * define via vitest.config.ts).
 */
export function defaultDaemonWsUrl(): string {
  return typeof __BSK_DAEMON_WS_URL__ === "string" ? __BSK_DAEMON_WS_URL__ : "ws://127.0.0.1:52800";
}

/**
 * Read the configured gateway WS endpoint. Falls back to the build-time
 * default `ws://127.0.0.1:52800`.
 */
export async function getDaemonWsUrl(
  storage: StorageBackend = defaultStorage(),
): Promise<string> {
  const items = await storage.get(DAEMON_WS_URL_KEY);
  const raw = items[DAEMON_WS_URL_KEY];
  return typeof raw === "string" && raw.trim() !== "" ? raw : defaultDaemonWsUrl();
}

/** Persist the gateway WS endpoint (e.g. `ws://192.168.10.99:52800`). */
export async function setDaemonWsUrl(
  url: string,
  storage: StorageBackend = defaultStorage(),
): Promise<void> {
  await storage.set({ [DAEMON_WS_URL_KEY]: url.trim() });
}

/** Read the extension token required by a token-enforcing gateway. */
export async function getExtensionToken(
  storage: StorageBackend = defaultStorage(),
): Promise<string> {
  const items = await storage.get(EXTENSION_TOKEN_KEY);
  const raw = items[EXTENSION_TOKEN_KEY];
  return typeof raw === "string" ? raw : "";
}

/** Persist the extension token used in `system.handshake`. */
export async function setExtensionToken(
  token: string,
  storage: StorageBackend = defaultStorage(),
): Promise<void> {
  await storage.set({ [EXTENSION_TOKEN_KEY]: token.trim() });
}

export const DAEMON_CONFIG_KEYS = {
  WS_URL: DAEMON_WS_URL_KEY,
  EXTENSION_TOKEN: EXTENSION_TOKEN_KEY,
} as const;