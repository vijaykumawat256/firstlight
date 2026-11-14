// @vitest-environment node
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const read = (p: string) => readFileSync(resolve(__dirname, "../../..", p), "utf8");

/** `api.something(` calls in a source file. */
const apiCalls = (src: string) => [...src.matchAll(/\bapi\.(\w+)\s*\(/g)].map((m) => m[1]!);

/** method name -> Tauri command, parsed from the real implementation so this can't drift. */
function commandOf(): Record<string, string> {
  const src = read("src/api/tauri.ts");
  const out: Record<string, string> = {};
  for (const m of src.matchAll(/(\w+):\s*(?:async\s*)?(?:\([^)]*\)|\w+)\s*=>\s*(?:invoke|openUrl|listen)[^"`]*["`]([\w-]+)["`]/g)) out[m[1]!] = m[2]!;
  return out;
}

describe("the up-next window's permissions match what its code does", () => {
  const caps = JSON.parse(read("src-tauri/capabilities/upnext.json")) as { permissions: Array<string | { identifier: string }> };
  const granted = new Set(caps.permissions.map((p) => (typeof p === "string" ? p : p.identifier)));
  // Calls that go through events or the opener plugin rather than a command.
  const viaPlugin: Record<string, string> = { onUpNextShown: "core:event:allow-listen", onSettingsChanged: "core:event:allow-listen", onUnlock: "core:event:allow-listen", openUrl: "opener:allow-open-url" };

  it("only calls things it is allowed to", () => {
    const map = commandOf();
    expect(Object.keys(map).length).toBeGreaterThan(30); // the parser found the real table
    const files = ["src/features/briefing/UpNextPanel.tsx", "src/features/settings/SettingsApplier.tsx", "src/features/settings/useSettings.ts"];
    const used = new Set(files.flatMap((f) => apiCalls(read(f))));
    // useSettings is shared with screens the panel never renders; only check what the panel's own tree calls.
    used.delete("setSettings");
    const missing: string[] = [];
    for (const method of used) {
      const need = viaPlugin[method] ?? (map[method] ? `allow-${map[method].replace(/_/g, "-")}` : undefined);
      if (!need) missing.push(`${method}: cannot tell which permission it needs`);
      else if (!granted.has(need)) missing.push(`${method} needs ${need}`);
    }
    expect(missing).toEqual([]);
  });

  it("is not granted anything that changes data", () => {
    for (const p of granted) expect(p).not.toMatch(/erase|set-settings|add-|remove|rename|import|enable|oauth|autostart/);
  });
});
