import { expect, test } from "@playwright/test";

// Motion stays ON in this file: the bug was in the opening animation.
test("a centred dialog is centred from the first frame of its opening animation, not only after it", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Privacy & data/ }).click();
  await page.getByRole("button", { name: "Erase…" }).click();
  const dialog = page.getByRole("dialog", { name: "Erase all data?" });
  await expect(dialog).toBeVisible();

  const centreAt = (progress: number) =>
    dialog.evaluate((el, p) => {
      const anim = el.getAnimations()[0];
      if (anim) {
        anim.pause();
        anim.currentTime = Number(anim.effect?.getComputedTiming().duration ?? 0) * p;
      }
      const r = el.getBoundingClientRect();
      return { x: r.left + r.width / 2, y: r.top + r.height / 2, vw: window.innerWidth, vh: window.innerHeight };
    }, progress);

  for (const p of [0, 0.5, 1]) {
    const c = await centreAt(p);
    expect(Math.abs(c.x - c.vw / 2), `horizontal centre at ${p * 100}% of the animation`).toBeLessThan(2);
    expect(Math.abs(c.y - c.vh / 2), `vertical centre at ${p * 100}% of the animation`).toBeLessThan(c.vh * 0.03);
  }
});
