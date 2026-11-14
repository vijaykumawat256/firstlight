import { CALDAV_PRESETS } from "./presets";

describe("CalDAV presets", () => {
  it("only point at https servers and help pages", () => {
    for (const p of CALDAV_PRESETS) {
      expect(new URL(p.server).protocol).toBe("https:");
      expect(new URL(p.helpUrl).protocol).toBe("https:");
    }
  });
  it("have unique ids", () => {
    expect(new Set(CALDAV_PRESETS.map((p) => p.id)).size).toBe(CALDAV_PRESETS.length);
  });
});
