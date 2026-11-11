import { cleanup, render } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Diagnostics from "./Diagnostics.svelte";

const metrics = {
  firmwareVersion: "2.2.0",
  resetReason: 1,
  uptime: 90061000,
  freeHeap: 68704,
  minFreeHeap: 58504,
  minFreeStack: 19368,
  lowHeapRefusals: 0,
  loopGapMaxUs: 71000,
  loopTickMaxUs: 850,
  loopLateTicks: 1,
  tsicGoodFrames: 0,
  tsicBadFrames: 0,
  tsicLastGoodMs: 0,
  ds18b20ReadErrors: 4,
  settingsSaves: 2,
  settingsSaveFailures: 0,
  settingsLastSave: 1,
  logBytes: 12409,
  wifiConnected: true,
  wifiRssi: -49,
  wifiReconnects: 0,
  mqttConnected: false,
};

type Listener = (event: { data: string }) => void;

/** Stands in for the browser's EventSource: tests drive it by hand. */
class FakeEventSource {
  static readonly CLOSED = 2;
  static instances: FakeEventSource[] = [];
  readyState = 0;
  closed = false;
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  private listeners: Record<string, Listener[]> = {};

  constructor(readonly url: string) {
    FakeEventSource.instances.push(this);
  }

  addEventListener(type: string, listener: Listener) {
    this.listeners[type] ??= [];
    this.listeners[type].push(listener);
  }

  emit(type: string, data: string) {
    for (const listener of this.listeners[type] ?? []) listener({ data });
  }

  /** A drop the browser retries by itself (readyState stays CONNECTING). */
  drop() {
    this.readyState = 0;
    this.onerror?.();
  }

  /** What the browser does when the server refuses the stream with a 503. */
  refuse() {
    this.readyState = FakeEventSource.CLOSED;
    this.onerror?.();
  }

  close() {
    this.closed = true;
    this.readyState = FakeEventSource.CLOSED;
  }
}

beforeEach(() => {
  FakeEventSource.instances = [];
  vi.stubGlobal("EventSource", FakeEventSource);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("Diagnostics page", () => {
  it("subscribes to the event stream with metrics requested", () => {
    const { queryByText } = render(Diagnostics);
    expect(queryByText(/No stream from the device/)).toBeNull();
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(FakeEventSource.instances[0].url).toMatch(/\/events\?metrics=1$/);
  });

  it("renders the counters from each metrics event", async () => {
    const { findByText, getByText } = render(Diagnostics);
    FakeEventSource.instances[0].emit("metrics", JSON.stringify(metrics));
    expect(await findByText("2.2.0")).toBeTruthy();
    expect(getByText("1d 01:01:01")).toBeTruthy();
    expect(getByText("67 KiB")).toBeTruthy();
    expect(getByText("71.0 ms")).toBeTruthy();
    expect(getByText("not connected")).toBeTruthy();
  });

  it("flags metrics that do not match the schema", async () => {
    const { findByText } = render(Diagnostics);
    FakeEventSource.instances[0].emit(
      "metrics",
      JSON.stringify({ firmwareVersion: "x" }),
    );
    expect(await findByText("Invalid metrics from the device.")).toBeTruthy();
  });

  it("flags a metrics event that is not JSON", async () => {
    const { findByText } = render(Diagnostics);
    FakeEventSource.instances[0].emit("metrics", "not-json");
    expect(await findByText("Invalid metrics from the device.")).toBeTruthy();
  });

  it("recovers from an invalid event once valid metrics arrive", async () => {
    const { findByText, queryByText } = render(Diagnostics);
    FakeEventSource.instances[0].emit("metrics", "not-json");
    await findByText("Invalid metrics from the device.");
    FakeEventSource.instances[0].emit("metrics", JSON.stringify(metrics));
    expect(await findByText("2.2.0")).toBeTruthy();
    expect(queryByText("Invalid metrics from the device.")).toBeNull();
  });

  it("shows a reconnect notice and reopens a refused stream", async () => {
    vi.useFakeTimers();
    const { findByText } = render(Diagnostics);
    FakeEventSource.instances[0].refuse();
    expect(await findByText(/No stream from the device/)).toBeTruthy();
    expect(FakeEventSource.instances).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(2000);
    expect(FakeEventSource.instances).toHaveLength(2);
  });

  it("leaves a browser-side retry to the browser", async () => {
    vi.useFakeTimers();
    const { queryByText } = render(Diagnostics);
    FakeEventSource.instances[0].drop();
    await vi.advanceTimersByTimeAsync(5000);
    expect(queryByText(/No stream from the device/)).toBeNull();
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  it("closes the stream on unmount and does not reopen it", async () => {
    vi.useFakeTimers();
    const { unmount } = render(Diagnostics);
    const first = FakeEventSource.instances[0];
    first.refuse();
    unmount();
    await vi.advanceTimersByTimeAsync(5000);
    expect(first.closed).toBe(true);
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  it("closes the reopened stream on unmount too", async () => {
    vi.useFakeTimers();
    const { unmount } = render(Diagnostics);
    FakeEventSource.instances[0].refuse();
    await vi.advanceTimersByTimeAsync(2000);
    const second = FakeEventSource.instances[1];
    unmount();
    expect(second.closed).toBe(true);
  });
});
