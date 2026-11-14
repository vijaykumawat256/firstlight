import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  reporter: "list",
  use: { baseURL: "http://localhost:1420", viewport: { width: 420, height: 680 }, deviceScaleFactor: 2 },
  webServer: [
    { command: "npm run dev", url: "http://localhost:1420", reuseExistingServer: true, timeout: 60_000 },
    // The production build, for tests that need the strict release CSP.
    { command: "npm run build && npx vite preview --port 4173 --strictPort", url: "http://localhost:4173", reuseExistingServer: true, timeout: 120_000 },
  ],
});
