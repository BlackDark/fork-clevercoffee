import { defineConfig, devices } from "@playwright/test";
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  workers: 1,
  timeout: 30000,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: "http://127.0.0.1:5173/ui/",
    trace: "retain-on-failure",
    launchOptions: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
      ? {
          executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH,
          args: ["--no-sandbox", "--disable-dev-shm-usage", "--disable-gpu"],
        }
      : {},
    screenshot: "only-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: [
    {
      command:
        "pnpm exec vite --mode mock --host 127.0.0.1 --port 5173 --strictPort",
      url: "http://127.0.0.1:5173/ui/",
      reuseExistingServer: !process.env.CI,
    },
    {
      command: "node --import tsx server.ts",
      cwd: "../mock-server",
      url: "http://localhost:3001/api/health",
      reuseExistingServer: !process.env.CI,
    },
  ],
});
