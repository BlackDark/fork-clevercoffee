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
  await expect(page.getByText("Online", { exact: true })).toBeVisible();
  await expect(page.getByText("Live", { exact: true })).toBeVisible();
  await expect(page.locator(".uplot")).toHaveCount(2);
  const temperature = page.getByLabel("Brew temperature", { exact: true });
  await expect(temperature).toBeEnabled();
  await temperature.fill("94");
  const request = page.waitForRequest(
    (r) => r.url().includes("/api/parameters") && r.method() === "POST",
  );
  await page.getByRole("button", { name: "Save", exact: true }).click();
  expect((await request).postData()).toBe("brew.setpoint=94");
  await expect(page.locator("[data-sonner-toast]")).toContainText(
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
    name: "Setpoint (°C)",
    exact: true,
  });
  const next = Number(await target.inputValue()) === 95 ? "96" : "95";
  await target.fill(next);
  await target.blur();
  await expect(
    page.getByRole("img", { name: "Edited", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("1 unsaved change", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("navigation", { name: "Parameter categories" })
    .getByRole("tab", { name: "System", exact: true })
    .click();
  await page.getByRole("tab", { name: "Behavior", exact: true }).click();
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
  await expect(
    page.locator("[data-sonner-toast][data-type=error]"),
  ).toContainText("Steam unavailable");
  await expect(toggle).toHaveAttribute("aria-checked", "false");
});

test("system requires confirmation and reports refused OTA updates", async ({
  page,
}, _testInfo) => {
  await page.route("**/api/ota/firmware", (route) =>
    route.fulfill({
      status: 409,
      json: { success: false, message: "Update refused while brewing" },
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
  await page.getByLabel("Firmware file (.bin)", { exact: true }).setInputFiles({
    name: "firmware.bin",
    mimeType: "application/octet-stream",
    buffer: Buffer.from([0xe9, 0, 0, 0]),
  });
  await page
    .getByRole("button", { name: "Upload update", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Install update", exact: true })
    .click();
  await expect(
    page.locator("[data-sonner-toast][data-type=error]"),
  ).toContainText("409");
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
  const mqtt = page.getByRole("switch", { name: "MQTT enabled", exact: true });
  await expect(mqtt).toBeVisible();
  await mqtt.check();
  const password = page.getByLabel("Password", { exact: true });
  await expect(password).toBeEnabled();
  await password.fill("private-test-secret");
  await password.blur();
  await page.getByRole("link", { name: "Home", exact: true }).click();
  await expect(
    page.getByRole("heading", {
      name: "Home",
      exact: true,
    }),
  ).toBeVisible();
  await page.getByRole("link", { name: "Configuration", exact: true }).click();
  await expect(
    page.getByRole("navigation", { name: "Parameter categories" }),
  ).toBeVisible();
  await page
    .getByRole("navigation", { name: "Parameter categories" })
    .getByRole("tab", { name: "System", exact: true })
    .click();
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
  await page.getByLabel("Firmware file (.bin)", { exact: true }).setInputFiles({
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
    page
      .locator("[data-sonner-toast]")
      .filter({ hasText: "Firmware accepted" }),
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
  await expect(
    page.locator("[data-sonner-toast][data-type=error]"),
  ).toBeVisible();
  expect(requests).toBe(0);
});

test("header navigation collapses on small screens and closes after selection or Escape", async ({
  page,
}) => {
  await page.setViewportSize({ width: 740, height: 900 });
  await page.goto("#/");
  const nav = page.getByRole("navigation", { name: "Main navigation" });
  const burger = page.getByRole("button", { name: "Open navigation menu" });
  await expect(nav).toBeHidden();
  await burger.click();
  await expect(nav).toBeVisible();
  await nav.getByRole("link", { name: "About", exact: true }).click();
  await expect(page).toHaveURL(/#\/about$/);
  await expect(nav).toBeHidden();
  await burger.click();
  await page.keyboard.press("Escape");
  await expect(nav).toBeHidden();
  await expect(burger).toBeFocused();
  await burger.click();
  await page.locator("main").click({ position: { x: 10, y: 500 } });
  await expect(nav).toBeHidden();
  await page.setViewportSize({ width: 1024, height: 900 });
  await expect(nav).toBeVisible();
  await expect(burger).toBeHidden();
  expect(await nav.evaluate((node) => node.closest("header") !== null)).toBe(
    true,
  );
  expect(await page.locator("aside").count()).toBe(0);
});

test("parameter cards, typography, notifications and system colors survive migration", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("#/config/behavior");
  const cards = page
    .locator(".parameter-panel")
    .first()
    .locator(".parameter-row");
  await expect(cards).toHaveCount(8);
  const first = await cards.nth(0).boundingBox();
  const second = await cards.nth(1).boundingBox();
  expect(first?.y).toBe(second?.y);
  expect(second?.x).toBeGreaterThan(first?.x ?? 0);
  const help = cards
    .first()
    .getByRole("button", { name: "Parameter help", exact: true });
  const tooltip = page.getByRole("tooltip");
  await expect(tooltip).toBeHidden();
  await help.hover();
  await expect(tooltip).toBeVisible();
  await page
    .getByRole("heading", { name: "Configuration", exact: true })
    .hover();
  await expect(tooltip).toBeHidden();
  await help.focus();
  await expect(tooltip).toBeVisible();
  await help.blur();
  await page.screenshot({
    path: testInfo.outputPath("parameters-desktop.png"),
    fullPage: true,
  });
  await page.getByRole("link", { name: "Home", exact: true }).click();
  const font = await page
    .locator(".maintenance-number")
    .evaluate((node) => getComputedStyle(node).fontFamily);
  expect(font).toBe(
    await page
      .locator("body")
      .evaluate((node) => getComputedStyle(node).fontFamily),
  );
  await page
    .getByRole("button", { name: "Reset counter", exact: true })
    .click();
  const toast = page
    .locator("[data-sonner-toast]")
    .filter({ hasText: "Device updated" });
  await expect(toast).toBeVisible();
  const bounds = await toast.boundingBox();
  expect(bounds?.x).toBeGreaterThan(1000);
  expect((bounds?.y ?? 0) + (bounds?.height ?? 0)).toBeGreaterThan(950);
  await toast.getByRole("button", { name: "Dismiss notification" }).click();
  await expect(toast).toBeHidden();
  await page.getByRole("link", { name: "System", exact: true }).click();
  await expect(page.getByRole("button", { name: "Wake up" })).toHaveCSS(
    "color",
    "rgb(255, 255, 255)",
  );
  await expect(page.getByRole("button", { name: "Wake up" })).not.toHaveCSS(
    "background-color",
    await page
      .getByRole("button", { name: "Factory reset", exact: true })
      .evaluate((node) => getComputedStyle(node).backgroundColor),
  );
  await page.screenshot({
    path: testInfo.outputPath("system-desktop.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "Switch to dark theme" }).click();
  await page.screenshot({
    path: testInfo.outputPath("system-dark.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("#/config/hardware");
  await expect(
    page.getByRole("heading", { name: "OLED Display", exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: testInfo.outputPath("parameters-mobile-dark.png"),
    fullPage: true,
  });
});

test("accepted counter reset succeeds and unavailable settings explain dependencies", async ({
  page,
}, testInfo) => {
  await page.route("**/api/maintenance/reset-backflush-counter", (route) =>
    route.fulfill({ status: 202, json: { accepted: true } }),
  );
  await page.goto("#/");
  await page
    .getByRole("button", { name: "Reset counter", exact: true })
    .click();
  await expect(
    page.locator("[data-sonner-toast]").filter({ hasText: "Device updated" }),
  ).toBeVisible();
  await expect(
    page.locator("[data-sonner-toast][data-type=error]"),
  ).toHaveCount(0);
  await page.goto("#/config/behavior");
  const card = page.locator(".parameter-row").filter({ hasText: "BD Kp" });
  const disabled = card.getByRole("button", {
    name: "Parameter disabled",
    exact: true,
  });
  await expect(disabled).toContainText("Disabled");
  await expect(card.getByRole("spinbutton")).toHaveCount(0);
  await disabled.hover();
  await expect(
    page.getByRole("tooltip").filter({ hasText: "Parameter disabled" }),
  ).toContainText("Enable Brew PID");
  await page.screenshot({
    path: testInfo.outputPath("disabled-parameter.png"),
  });
});

test("blue controls, centered disabled badges and compact maintenance placement", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("#/");
  const maintenance = page.locator(".maintenance-panel");
  const status = page.locator(".home-grid > .panel").first();
  await expect(maintenance).toBeVisible();
  const box = await maintenance.boundingBox();
  const statusBox = await status.boundingBox();
  const chartBox = await page.locator(".chart-panel").first().boundingBox();
  expect(box?.y).toBe(statusBox?.y);
  expect(box?.width).toBe(statusBox?.width);
  expect(chartBox?.y).toBeGreaterThan((box?.y ?? 0) + (box?.height ?? 0));
  expect(
    await page
      .locator("html")
      .evaluate((node) =>
        getComputedStyle(node).getPropertyValue("--primary").trim(),
      ),
  ).toBe("#2563eb");
  const heating = page.getByRole("switch", {
    name: "PID heating",
    exact: true,
  });
  await expect(heating).toHaveAttribute("data-slot", "switch");
  await expect(heating).toHaveCSS("background-color", "rgb(37, 99, 235)");
  const track = await heating.boundingBox();
  const thumb = await heating.locator("[data-slot=switch-thumb]").boundingBox();
  expect(
    Math.abs(
      (track?.x ?? 0) +
        (track?.width ?? 0) -
        (thumb?.x ?? 0) -
        (thumb?.width ?? 0),
    ),
  ).toBeLessThan(4);
  await page.screenshot({ path: testInfo.outputPath("dashboard-blue.png") });
  await page.goto("#/config/behavior");
  const card = page.locator(".parameter-row").filter({ hasText: "BD Kp" });
  const badge = card.getByRole("button", {
    name: "Parameter disabled",
    exact: true,
  });
  const cardBox = await card.boundingBox();
  const badgeBox = await badge.boundingBox();
  expect(
    Math.abs(
      (badgeBox?.x ?? 0) +
        (badgeBox?.width ?? 0) / 2 -
        (cardBox?.x ?? 0) -
        (cardBox?.width ?? 0) / 2,
    ),
  ).toBeLessThan(2);
  const select = page.getByLabel("Display Language", { exact: true });
  await expect(select).toHaveAttribute("data-slot", "select-trigger");
  await page.goto("#/config/hardware");
  await expect(page.getByRole("alert")).toContainText(
    "Incorrect hardware settings",
  );
  await page.getByRole("button", { name: "Switch to dark theme" }).click();
  expect(
    await page
      .locator("html")
      .evaluate((node) =>
        getComputedStyle(node).getPropertyValue("--primary").trim(),
      ),
  ).toBe("#60a5fa");
});

test("select labels survive repeated changes, zero values, keyboard selection and navigation", async ({
  page,
}, testInfo) => {
  await page.route("**/api/parameters?filter=all", async (route) => {
    const response = await route.fetch();
    const parameters = await response.json();
    for (const parameter of parameters) {
      if (parameter.name === "hardware.switches.brew.enabled")
        parameter.value = true;
      if (parameter.name === "brew.mode") parameter.value = 1;
    }
    await route.fulfill({ json: parameters });
  });
  await page.goto("#/config/behavior");
  const select = page.getByLabel("Brew Mode", { exact: true });
  await expect(select).toHaveText("Automatic");
  await select.click();
  await expect(page.getByRole("listbox")).toBeVisible();
  await page.getByRole("option", { name: "Manual", exact: true }).click();
  await expect(select).toHaveText("Manual");
  await select.click();
  await page.getByRole("option", { name: "Automatic", exact: true }).click();
  await expect(select).toHaveText("Automatic");
  await select.focus();
  await select.press("ArrowDown");
  await select.press("Home");
  await select.press("Enter");
  await expect(select).toHaveText("Manual");
  await page.goto("#/config/system");
  await page.goto("#/config/behavior");
  await expect(select).toHaveText("Manual");
  await page.getByRole("button", { name: "Switch to dark theme" }).click();
  await select.click();
  await page.screenshot({ path: testInfo.outputPath("select-dark.png") });
  await page.keyboard.press("Escape");
  await expect(select).toHaveText("Manual");
  await expect(select).toBeFocused();
});

test("warnings align icons beside text and System/About share the page container", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("#/");
  const home = await page.locator("main").boundingBox();
  await page.goto("#/system");
  const system = await page.locator("main").boundingBox();
  expect(system?.x).toBe(home?.x);
  expect(system?.width).toBe(home?.width);
  const buttons = page.locator("main [data-slot=button]");
  expect(await buttons.count()).toBeGreaterThan(5);
  for (const button of await buttons.all())
    await expect(button.locator("svg")).toHaveCount(1);
  const warning = page
    .getByRole("alert")
    .filter({ hasText: "Keep the device powered" });
  const icon = await warning.locator(":scope > svg").boundingBox();
  const description = await warning
    .locator("[data-slot=alert-description]")
    .boundingBox();
  expect((icon?.x ?? 0) + (icon?.width ?? 0)).toBeLessThan(description?.x ?? 0);
  expect(Math.abs((icon?.y ?? 0) - (description?.y ?? 0))).toBeLessThan(5);
  await page.screenshot({
    path: testInfo.outputPath("system-aligned.png"),
    fullPage: true,
  });
  await page.goto("#/about");
  const about = await page.locator("main").boundingBox();
  expect(about?.x).toBe(home?.x);
  expect(about?.width).toBe(home?.width);
  const card = await page.locator("main > [data-slot=card]").boundingBox();
  expect(card?.width).toBe((about?.width ?? 0) - 48);
  await page.screenshot({
    path: testInfo.outputPath("about-aligned.png"),
    fullPage: true,
  });
  await page.goto("#/config/hardware");
  const hardware = page
    .getByRole("alert")
    .filter({ hasText: "Incorrect hardware settings" });
  await expect(hardware.locator(":scope > svg")).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  const mobileIcon = await hardware.locator(":scope > svg").boundingBox();
  const mobileText = await hardware
    .locator("[data-slot=alert-description]")
    .boundingBox();
  expect((mobileIcon?.x ?? 0) + (mobileIcon?.width ?? 0)).toBeLessThan(
    mobileText?.x ?? 0,
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});

for (const response of [
  {
    status: 413,
    body: "Upload too large",
    expected: "HTTP 413: Upload too large",
  },
  {
    status: 500,
    body: "Storage failure",
    expected: "HTTP 500: Storage failure",
  },
  {
    status: 200,
    body: "not json",
    expected: "Invalid configuration upload response (HTTP 200)",
  },
  {
    status: 200,
    body: "{}",
    expected: "Invalid configuration upload response (HTTP 200)",
  },
]) {
  test(`configuration restore explains ${response.status} response ${response.body}`, async ({
    page,
  }) => {
    await page.route("**/api/config/upload", (route) =>
      route.fulfill({
        status: response.status,
        contentType: "text/plain",
        body: response.body,
      }),
    );
    await page.goto("#/system");
    await page.getByLabel("Configuration file", { exact: true }).setInputFiles({
      name: "config.json",
      mimeType: "application/json",
      buffer: Buffer.from("{}"),
    });
    await page
      .getByRole("button", { name: "Restore configuration", exact: true })
      .click();
    await page.getByRole("button", { name: "Continue", exact: true }).click();
    await expect(
      page.locator("[data-sonner-toast][data-type=error]"),
    ).toContainText(response.expected);
  });
}

test("configuration restore enforces the 8 KB device limit", async ({
  page,
}) => {
  let requests = 0;
  await page.route("**/api/config/upload", (route) => {
    requests++;
    return route.fulfill({ json: { success: true } });
  });
  await page.goto("#/system");
  const file = page.getByLabel("Configuration file", { exact: true });
  for (const size of [8193, 8192]) {
    const text = JSON.stringify({ padding: "x".repeat(size - 14) });
    expect(Buffer.byteLength(text)).toBe(size);
    await file.setInputFiles({
      name: "config.json",
      mimeType: "application/json",
      buffer: Buffer.from(text),
    });
    await page
      .getByRole("button", { name: "Restore configuration", exact: true })
      .click();
    await page.getByRole("button", { name: "Continue", exact: true }).click();
    if (size > 8192) {
      await expect(
        page.locator("[data-sonner-toast][data-type=error]"),
      ).toContainText("up to 8 KB");
      expect(requests).toBe(0);
      await page.getByRole("button", { name: "Dismiss notification" }).click();
    } else {
      await expect(
        page.locator("[data-sonner-toast][data-type=success]"),
      ).toBeVisible();
      expect(requests).toBe(1);
    }
  }
});

test("unknown device parameters without bounds remain editable", async ({
  page,
}) => {
  await page.route("**/api/parameters?filter=all", async (route) => {
    const response = await route.fetch();
    const parameters = await response.json();
    parameters.push({
      name: "device.future",
      type: 2,
      value: 12,
      min: null,
      max: null,
    });
    await route.fulfill({ json: parameters });
  });
  await page.goto("#/config/system");
  const input = page.getByRole("spinbutton", {
    name: "device.future",
    exact: true,
  });
  await input.fill("42");
  await input.blur();
  await page
    .getByRole("button", { name: "Review & save", exact: true })
    .click();
  const request = page.waitForRequest(
    (r) => r.url().endsWith("/api/parameters") && r.method() === "POST",
  );
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  expect((await request).postData()).toBe("device.future=42");
});
