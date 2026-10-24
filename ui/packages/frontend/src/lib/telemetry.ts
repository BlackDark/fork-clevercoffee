import type { HistoryData, TemperatureData } from "./schemas";
export const MAX_HISTORY_POINTS = 1200;
export type ChartData = [number[], number[], number[], number[]];
export function historyToChart(
  history: HistoryData,
  now = Date.now(),
): ChartData {
  const count = Math.min(
    history.currentTemps.length,
    history.targetTemps.length,
    history.heaterPowers.length,
  );
  const start = Math.max(0, count - MAX_HISTORY_POINTS);
  return [
    Array.from(
      { length: count - start },
      (_, i) => now / 1000 - (count - start - i) * 3,
    ),
    history.currentTemps.slice(start, count),
    history.targetTemps.slice(start, count),
    history.heaterPowers.slice(start, count),
  ];
}
export function appendSample(
  chart: ChartData,
  sample: TemperatureData,
  now = Date.now(),
): ChartData {
  const values = [
    Math.max(now / 1000, (chart[0].at(-1) ?? 0) + 0.001),
    sample.currentTemp,
    sample.targetTemp,
    sample.heaterPower,
  ];
  return chart.map((series, i) => [
    ...series.slice(-(MAX_HISTORY_POINTS - 1)),
    values[i],
  ]) as ChartData;
}
