import { describe, expect, it } from "vitest";
import { appendSample, historyToChart, MAX_HISTORY_POINTS } from "./telemetry";

describe("bounded telemetry history", () => {
  it("aligns unequal history arrays and limits the retained window", () => {
    const currentTemps = Array.from({ length: 1500 }, (_, i) => i);
    const chart = historyToChart(
      {
        currentTemps,
        targetTemps: currentTemps.slice(0, 1450),
        heaterPowers: currentTemps.slice(0, 1400),
      },
      10000000,
    );
    expect(chart.map((s) => s.length)).toEqual([1200, 1200, 1200, 1200]);
    expect(chart[1][0]).toBe(200);
    expect(chart[0].at(-1)).toBe(9997);
  });
  it("keeps all series bounded and timestamps increasing when the clock moves backwards", () => {
    let chart = historyToChart({
      currentTemps: [],
      targetTemps: [],
      heaterPowers: [],
    });
    for (let i = 0; i < 1300; i++)
      chart = appendSample(
        chart,
        { currentTemp: i, targetTemp: 93, heaterPower: 50 },
        10000 - i,
      );
    expect(chart.map((s) => s.length)).toEqual(
      Array(4).fill(MAX_HISTORY_POINTS),
    );
    expect(chart[0].every((v, i) => i === 0 || v > chart[0][i - 1])).toBe(true);
    expect(chart[1].at(-1)).toBe(1299);
  });
});
