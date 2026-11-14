import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  reporter: "list",
  use: { baseURL: "http://localhost:1420", viewport: { width: 420, height: 680 }, deviceScaleFactor: 2 },
  webServer: { command: "npm run dev", url: "http://localhost:1420", reuseExistingServer: true, timeout: 60_000 },
});
