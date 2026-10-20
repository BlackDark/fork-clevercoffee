import { expect, test } from "@playwright/test";

declare global {
  interface Window {
    __receivedSamples: number;
  }
}

let runtimeErrors: string[] = [];
test.beforeEach(async ({ page }) => {
  runtimeErrors = [];
  page.on("pageerror", (error) => runtimeErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "warning" && message.text().includes("[svelte]"))
      runtimeErrors.push(message.text());
  });
});
test.afterEach(() => expect(runtimeErrors).toEqual([]));

test("dashboard shows live telemetry, retains charts, and sends only the edited setpoint", async ({
  page,
}, testInfo) => {
  await page.addInitScript(() => {
    const tracked = window;
    tracked.__receivedSamples = 0;
    const NativeEventSource = window.EventSource;
    window.EventSource = class extends NativeEventSource {
      constructor(url: string | URL, init?: EventSourceInit) {
        super(url, init);
        this.addEventListener("new_temps", () => {
          tracked.__receivedSamples++;
        });
      }
    };
  });
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("#/error");
  await page.getByRole("link", { name: "Back to dashboard" }).click();
  await expect(
    page.getByText("Device connected", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("Live", { exact: true })).toBeVisible();
  await expect(page.locator(".uplot")).toHaveCount(2);
  const temperature = page.getByLabel("Brew temperature", { exact: true });
  await expect(temperature).toBeEnabled();
  await temperature.fill("94");
  const request = page.waitForRequest(
    (r) => r.url().includes("/api/parameters") && r.method() === "POST",
  );
  await page.getByRole("button", { name: "Set", exact: true }).click();
  expect((await request).postData()).toBe("brew.setpoint=94");
  await expect(page.getByRole("status")).toContainText(
    "Brew temperature saved",
  );
  const charts = await page.locator(".uplot").evaluateAll((nodes) => {
    for (const node of nodes) node.setAttribute("data-instance", "original");
    return nodes.length;
  });
  const before = await page.evaluate(() => window.__receivedSamples);
  await expect
    .poll(() => page.evaluate(() => window.__receivedSamples))
    .toBeGreaterThan(before);
  await expect
    .poll(() => page.locator('.uplot[data-instance="original"]').count(), {
      timeout: 5000,
    })
    .toBe(charts);
  await page.screenshot({
    path: testInfo.outputPath("dashboard-desktop.png"),
    fullPage: true,
  });
  expect(errors).toEqual([]);
});

test("parameter editing preserves drafts across categories and masks secrets in review", async ({
  page,
}, _testInfo) => {
  await page.goto("#/config/behavior");
  await expect(
    page.getByRole("button", { name: "Review & save" }),
  ).toBeDisabled();
  const target = page.getByRole("spinbutton", {
    name: "brew.setpoint",
    exact: true,
  });
  const next = Number(await target.inputValue()) === 95 ? "96" : "95";
  await target.fill(next);
  await target.blur();
  await expect(
    page.getByText("1 unsaved change", { exact: true }),
  ).toBeVisible();
  await page.getByRole("link", { name: "System", exact: true }).last().click();
  await page.getByRole("link", { name: "Behavior", exact: true }).click();
  await expect(target).toHaveValue(next);
  await page.getByRole("button", { name: "Review & save" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  const request = page.waitForRequest(
    (r) => r.url().includes("/api/parameters") && r.method() === "POST",
  );
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  expect((await request).postData()).toBe(`brew.setpoint=${next}`);
  await expect(
    page.getByText("All changes saved", { exact: true }),
  ).toBeVisible();
});

test("commands show the device's state and rejection messages", async ({
  page,
}, _testInfo) => {
  await page.route("**/api/steam", (route) =>
    route.fulfill({ json: { success: false, message: "Steam unavailable" } }),
  );
  await page.goto("#/");
  const toggle = page.getByRole("switch", { name: "Steam mode" });
  await expect(toggle).toBeEnabled();
  await toggle.click();
  await expect(page.getByRole("alert")).toContainText("Steam unavailable");
  await expect(toggle).toHaveAttribute("aria-checked", "false");
});

test("system requires confirmation and reports unsupported OTA requests", async ({
  page,
}, _testInfo) => {
  await page.route("**/api/ota/url", (route) =>
    route.fulfill({
      status: 501,
      json: { success: false, message: "URL updates are not implemented" },
    }),
  );
  await page.goto("#/system");
  await page
    .getByRole("button", { name: "Factory reset", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toContainText(
    "Erase all saved settings",
  );
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page
    .getByLabel("Update from URL", { exact: true })
    .fill("https://example.com/firmware.bin");
  await expect(
    page.getByRole("button", { name: "Update from URL", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Update from URL", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Install update", exact: true })
    .click();
  await expect(page.getByRole("status")).toContainText("501");
});

test("mobile layout has no horizontal overflow and supports dark theme", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("#/");
  await expect(page.locator(".uplot")).toHaveCount(2);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: testInfo.outputPath("dashboard-mobile.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "Switch to dark theme" }).click();
  await expect(page.locator("html")).toHaveClass("dark");
  await page.reload();
  await expect(page.locator("html")).toHaveClass("dark");
  await page.setViewportSize({ width: 1440, height: 1100 });
  await expect(page.locator(".uplot")).toHaveCount(2);
  await page.screenshot({
    path: testInfo.outputPath("dashboard-dark.png"),
    fullPage: true,
  });
});

test("drafts survive dashboard navigation and secrets stay masked in review", async ({
  page,
}) => {
  await page.goto("#/config/system");
  const mqtt = page.getByRole("switch", { name: "mqtt.enabled", exact: true });
  await expect(mqtt).toBeVisible();
  await mqtt.check();
  const password = page.getByLabel("mqtt.password", { exact: true });
  await expect(password).toBeEnabled();
  await password.fill("private-test-secret");
  await password.blur();
  await page.getByRole("link", { name: "Dashboard", exact: true }).click();
  await page.getByRole("link", { name: "Parameters", exact: true }).click();
  await page.getByRole("link", { name: "System", exact: true }).last().click();
  await expect(password).toHaveValue("private-test-secret");
  await page.getByRole("button", { name: "Review & save" }).click();
  await expect(page.getByRole("dialog")).not.toContainText(
    "private-test-secret",
  );
  await expect(page.getByRole("dialog")).toContainText("••••••");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("button", { name: "Discard", exact: true }).click();
});

test("offline state disables machine toggles", async ({ page }) => {
  await page.route("**/api/status", (route) =>
    route.fulfill({ status: 503, json: { message: "Offline" } }),
  );
  await page.goto("#/");
  await expect(page.getByRole("alert")).toContainText(
    "Cannot reach the device",
  );
  await expect(page.getByRole("switch", { name: "Steam mode" })).toBeDisabled();
});

test("OTA file upload preserves the multipart contract", async ({ page }) => {
  await page.route("**/api/ota/firmware", (route) =>
    route.fulfill({ json: { success: true, message: "Firmware accepted" } }),
  );
  await page.goto("#/system");
  await page.getByLabel("Upload a file", { exact: true }).setInputFiles({
    name: "firmware.bin",
    mimeType: "application/octet-stream",
    buffer: Buffer.from("test firmware"),
  });
  await expect(
    page.getByRole("button", { name: "Upload update", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Upload update", exact: true })
    .click();
  const request = page.waitForRequest(
    (r) => r.url().endsWith("/api/ota/firmware") && r.method() === "POST",
  );
  await page
    .getByRole("button", { name: "Install update", exact: true })
    .click();
  const sent = await request;
  expect(sent.headers()["content-type"]).toContain(
    "multipart/form-data; boundary=",
  );
  expect(sent.postDataBuffer()?.toString()).toContain(
    'name="firmware"; filename="firmware.bin"',
  );
  await expect(
    page.getByRole("status").filter({ hasText: "Firmware accepted" }),
  ).toBeVisible();
});

test("invalid configuration is rejected before upload", async ({ page }) => {
  let requests = 0;
  await page.route("**/api/config/upload", (route) => {
    requests++;
    return route.fulfill({ json: { success: true } });
  });
  await page.goto("#/system");
  await page.getByLabel("Configuration file", { exact: true }).setInputFiles({
    name: "config.json",
    mimeType: "application/json",
    buffer: Buffer.from("not json"),
  });
  await page
    .getByRole("button", { name: "Restore configuration", exact: true })
    .click();
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  expect(requests).toBe(0);
});
