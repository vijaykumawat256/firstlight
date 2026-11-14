import { timeAgo } from "./time";

describe("timeAgo", () => {
  const now = new Date("2026-03-10T12:00:00Z");
  const ago = (ms: number) => new Date(now.getTime() - ms).toISOString();
  it("describes recency in plain words", () => {
    expect(timeAgo(null, now)).toBe("never");
    expect(timeAgo(ago(20_000), now)).toBe("just now");
    expect(timeAgo(ago(5 * 60_000), now)).toBe("5 min ago");
    expect(timeAgo(ago(3 * 3_600_000), now)).toBe("3 h ago");
    expect(timeAgo(ago(50 * 3_600_000), now)).toBe("2 d ago");
  });
  it("never goes negative for clock skew", () => {
    expect(timeAgo(new Date(now.getTime() + 60_000).toISOString(), now)).toBe("just now");
  });
});
