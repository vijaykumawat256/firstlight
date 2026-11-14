import type { Backend } from "./backend";
import { fixtureBackend } from "./fixture";
import { tauriBackend } from "./tauri";

export { errorMessage, type Backend } from "./backend";
export * from "./types";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** The one entry point the UI uses to talk to the outside world. */
export const api: Backend = isTauri ? tauriBackend : fixtureBackend;
