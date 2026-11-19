import { describe, expect, it } from "vitest";
import { allTemplates, BUILT_IN_TEMPLATES, templateFor } from "./templates";

describe("templates", () => {
  const all = allTemplates([{ name: "Standup", minutes: 10, agenda: "Mine" }, { name: "Design crit", minutes: 45, agenda: "" }]);

  it("yours replace a built-in one with the same name", () => {
    expect(all.filter((t) => t.name === "Standup")).toEqual([{ name: "Standup", minutes: 10, agenda: "Mine" }]);
    expect(all).toHaveLength(BUILT_IN_TEMPLATES.length + 1);
  });

  it("are picked from the start of a title, whole words only", () => {
    expect(templateFor("1:1 with Sam", all)?.name).toBe("1:1");
    expect(templateFor("standup", all)?.minutes).toBe(10);
    expect(templateFor("Design crit: homepage", all)?.name).toBe("Design crit");
    expect(templateFor("Interviewing tips", all)).toBeNull();
    expect(templateFor("Lunch", all)).toBeNull();
  });
});
