import { layoutColumn, minutesOfDay, sameDay, startOfLocalDay, startOfWeek } from "./layout";

const day = new Date(2026, 2, 10); // Tuesday 10 March 2026
const t = (h: number, m = 0, dayOffset = 0) => new Date(2026, 2, 10 + dayOffset, h, m);
const blk = (id: string, s: Date, e: Date) => ({ id, start: s, end: e });
const byId = (p: ReturnType<typeof layoutColumn>) => Object.fromEntries(p.map((x) => [x.id, x]));

describe("layoutColumn", () => {
  it("gives separate events the full width", () => {
    const p = byId(layoutColumn([blk("a", t(9), t(10)), blk("b", t(10), t(11))], day));
    expect(p.a).toMatchObject({ top: 540, height: 60, col: 0, cols: 1 });
    expect(p.b).toMatchObject({ top: 600, col: 0, cols: 1 });
  });

  it("splits overlapping events into side-by-side columns", () => {
    const p = byId(layoutColumn([blk("a", t(10), t(11, 30)), blk("b", t(11), t(12))], day));
    expect([p.a!.col, p.b!.col]).toEqual([0, 1]);
    expect([p.a!.cols, p.b!.cols]).toEqual([2, 2]);
  });

  it("handles a chain: A overlaps B, B overlaps C, A does not overlap C", () => {
    const p = byId(layoutColumn([blk("a", t(9), t(10)), blk("b", t(9, 30), t(10, 30)), blk("c", t(10), t(11))], day));
    expect(p.a).toMatchObject({ col: 0, cols: 2 });
    expect(p.b).toMatchObject({ col: 1, cols: 2 });
    expect(p.c).toMatchObject({ col: 0, cols: 2 }); // reuses A's column once A has ended
  });

  it("a long event keeps its column while shorter ones stack beside it", () => {
    const p = byId(layoutColumn([blk("long", t(9), t(17)), blk("x", t(10), t(11)), blk("y", t(12), t(13))], day));
    expect(p.long).toMatchObject({ col: 0, cols: 2 });
    expect(p.x!.col).toBe(1);
    expect(p.y!.col).toBe(1);
  });

  it("starts a new cluster after a gap so earlier overlaps don't squeeze later events", () => {
    const p = byId(layoutColumn([blk("a", t(9), t(10)), blk("b", t(9), t(10)), blk("c", t(14), t(15))], day));
    expect(p.a!.cols).toBe(2);
    expect(p.c).toMatchObject({ col: 0, cols: 1 });
  });

  it("clips an event that crosses midnight to each day", () => {
    const night = blk("n", t(22), t(2, 0, 1));
    const today = layoutColumn([night], day)[0]!;
    expect([today.top, today.height]).toEqual([1320, 120]);
    const tomorrow = layoutColumn([night], startOfLocalDay(day, 1))[0]!;
    expect([tomorrow.top, tomorrow.height]).toEqual([0, 120]);
    expect(layoutColumn([night], startOfLocalDay(day, 2))).toEqual([]);
  });

  it("keeps very short events tall enough to click", () => {
    expect(layoutColumn([blk("s", t(9), t(9, 5))], day)[0]!.height).toBe(20);
    expect(layoutColumn([blk("z", t(9), t(9))], day)[0]!.height).toBe(20);
  });

  it("is deterministic regardless of input order", () => {
    const a = [blk("a", t(9), t(10)), blk("b", t(9), t(10)), blk("c", t(9, 30), t(11))];
    expect(layoutColumn([...a].reverse(), day)).toEqual(layoutColumn(a, day));
  });
});

describe("minutesOfDay", () => {
  it("uses the wall clock and clamps to the day", () => {
    expect(minutesOfDay(t(9, 30), day)).toBe(570);
    expect(minutesOfDay(t(9, 30, -1), day)).toBe(0);
    expect(minutesOfDay(t(9, 30, 1), day)).toBe(1440);
  });
});

describe("week helpers", () => {
  it("finds the start of the week for either convention", () => {
    expect(startOfWeek(day, 1).getDate()).toBe(9); // Monday
    expect(startOfWeek(day, 0).getDate()).toBe(8); // Sunday
    expect(startOfWeek(new Date(2026, 2, 9), 1).getDate()).toBe(9); // already Monday
    expect(startOfWeek(new Date(2026, 2, 8), 1).getDate()).toBe(2); // Sunday belongs to the previous Monday's week
  });
  it("crosses month and year boundaries", () => {
    expect(startOfWeek(new Date(2026, 0, 1), 1)).toEqual(new Date(2025, 11, 29));
  });
  it("compares calendar days", () => {
    expect(sameDay(t(1), t(23))).toBe(true);
    expect(sameDay(t(23), t(0, 0, 1))).toBe(false);
  });
});
