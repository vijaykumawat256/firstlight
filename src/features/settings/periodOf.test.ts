import { describe, expect, it } from "vitest";
import { periodOf } from "./SettingsApplier";

describe("periodOf", () => {
  const at = (h: number, m = 0) => new Date(2026, 10, 14, h, m);
  it("splits the day into morning, day, evening and night", () => {
    expect(periodOf(at(4, 59))).toBe("night");
    expect(periodOf(at(5))).toBe("morning");
    expect(periodOf(at(10, 59))).toBe("morning");
    expect(periodOf(at(11))).toBe("day");
    expect(periodOf(at(16, 59))).toBe("day");
    expect(periodOf(at(17))).toBe("evening");
    expect(periodOf(at(20, 59))).toBe("evening");
    expect(periodOf(at(21))).toBe("night");
    expect(periodOf(at(0))).toBe("night");
  });
});
