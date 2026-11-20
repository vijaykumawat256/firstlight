import type { Page } from "@playwright/test";

/**
 * Waits until nothing on the page is animating (dialog fades, hover transitions), so axe never samples a half-faded
 * colour. Repeats because one fade can start just after another ends (a dialog opening as the previous one closes).
 * Not waitForFunction or requestAnimationFrame: tests with a faked clock never advance those; a MessageChannel
 * message is a real task, so it yields to the page without the clock.
 */
export async function settle(page: Page) {
  await page.evaluate(async () => {
    const tick = () =>
      new Promise<void>((resolve) => {
        const ch = new MessageChannel();
        ch.port1.onmessage = () => resolve();
        ch.port2.postMessage(null);
      });
    for (let round = 0; round < 20; round++) {
      await tick();
      const running = document
        .getAnimations()
        .filter((a) => (a.playState === "running" || a.pending) && a.effect?.getComputedTiming().iterations !== Infinity);
      if (running.length === 0) return;
      await Promise.all(running.map((a) => a.finished.catch(() => undefined)));
    }
  });
}
