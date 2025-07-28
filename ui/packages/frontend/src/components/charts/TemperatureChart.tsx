import { useCallback } from "react";
import { BaseChart } from "./BaseChart";
import uPlot from "uplot";
import { getChartTheme } from "@/hooks/use-dark-mode";

export interface TemperatureChartProps {
  data: number[][];
  width?: number;
  height?: number;
  title?: string;
  tempRange?: [number, number];
  autoScaleY?: boolean;
  onError?: (error: Error) => void;
}

const tzdateOptions: Intl.DateTimeFormatOptions = {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
};

export function TemperatureChart({
  data,
  width,
  height = 400,
  title = "Temperature History",
  tempRange,
  autoScaleY = true,
  onError,
}: TemperatureChartProps) {
  // Chart config
  const createChartConfig = useCallback(
    ({
      chartSize,
      theme,
    }: {
      chartSize: { width: number; height: number };
      theme: ReturnType<typeof getChartTheme>;
    }) => ({
      title,
      width: chartSize.width,
      height: chartSize.height,
      tzDate: (ts: number) => new Date(ts * 1000),
      series: [
        {},
        {
          label: "Current Temperature",
          scale: "°C",
          value: (_u: uPlot, v: number) =>
            v == null ? "" : v.toFixed(1) + " °C",
          show: true,
          stroke: theme.series.temperature.current,
          fill: theme.series.temperature.current + "20",
          width: 2,
          points: { show: false },
        },
        {
          label: "Target Temperature",
          scale: "°C",
          value: (_u: uPlot, v: number) =>
            v == null ? "" : v.toFixed(1) + " °C",
          stroke: theme.series.temperature.target,
          fill: theme.series.temperature.target + "20",
          width: 2,
          show: true,
          points: { show: false },
        },
      ],
      axes: [
        {
          values: (_u: uPlot, vals: number[]) =>
            vals.map((v) =>
              new Date(v * 1000).toLocaleString("de-DE", tzdateOptions)
            ),
          stroke: theme.axisColor,
          grid: { stroke: theme.gridColor },
        },
        {
          scale: "°C",
          values: (_u: uPlot, vals: number[]) => vals.map((v) => v + "°C"),
          stroke: theme.axisColor,
          grid: { stroke: theme.gridColor },
        },
      ],
      scales: {
        "°C": {
          auto: autoScaleY,
          ...(autoScaleY === false && tempRange ? { range: tempRange } : {}),
        },
        x: {
          auto: true,
        },
      },
      cursor: {
        drag: {
          x: true,
          y: false,
        },
      },
    }),
    [title, autoScaleY, tempRange]
  );

  // Data update logic
  const updatePlotData = useCallback(
    (plot: uPlot, newData: number[][]) => {
      try {
        plot.setData(newData as uPlot.AlignedData);
        if (!autoScaleY && tempRange) {
          plot.setScale("°C", { min: tempRange[0], max: tempRange[1] });
        }
      } catch (error) {
        if (onError)
          onError(error instanceof Error ? error : new Error(String(error)));
      }
    },
    [autoScaleY, tempRange, onError]
  );

  return (
    <BaseChart
      data={data}
      width={width}
      height={height}
      title={title}
      onError={onError}
      createChartConfig={createChartConfig}
      updatePlotData={updatePlotData}
      minHeight={height}
      ariaLabel={`${title} chart showing temperature over time`}
    />
  );
}

export default TemperatureChart;
