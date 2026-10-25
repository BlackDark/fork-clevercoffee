import { cleanup, render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Chart from "./Chart.svelte";

const chart = vi.hoisted(() => ({
  create: vi.fn(),
  destroy: vi.fn(),
  setData: vi.fn(),
  setSize: vi.fn(),
}));
vi.mock("uplot", () => ({
  default: class {
    width = 240;
    destroy = chart.destroy;
    setData = chart.setData;
    setSize = chart.setSize;
    constructor() {
      chart.create();
    }
  },
}));
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("constructs once on mount and rebuilds only for a changed theme", async () => {
  const { rerender } = render(Chart, { data: [[1], [20], [95]], dark: false });
  await tick();
  expect(chart.create).toHaveBeenCalledTimes(1);
  await rerender({ data: [[2], [21], [95]], dark: false });
  expect(chart.create).toHaveBeenCalledTimes(1);
  await rerender({ data: [[2], [21], [95]], dark: true });
  expect(chart.create).toHaveBeenCalledTimes(2);
  expect(chart.destroy).toHaveBeenCalledTimes(1);
});

it("reports a theme rebuild failure instead of throwing out of the effect", async () => {
  const { rerender, getByRole } = render(Chart, {
    data: [[1], [20], [95]],
    dark: false,
  });
  await tick();
  chart.create.mockImplementationOnce(() => {
    throw new Error("chart build failed");
  });
  await rerender({ data: [[1], [20], [95]], dark: true });
  await waitFor(() =>
    expect(getByRole("alert")).toHaveTextContent("chart build failed"),
  );
});
