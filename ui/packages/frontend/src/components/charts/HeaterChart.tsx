import { useCallback } from "react";
import { BaseChart } from "./BaseChart";
import uPlot from "uplot";
import type { getChartTheme } from "@/hooks/use-dark-mode";

export interface HeaterChartProps {
  data: number[][];
  width?: number;
  height?: number;
  title?: string;
  yRange?: [number, number];
  onError?: (error: Error) => void;
}

const tzdateOptions: Intl.DateTimeFormatOptions = {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
};

export function HeaterChart({
  data,
  width,
  height = 400,
  title = "Heater Power History",
  yRange = [0, 105],
  onError,
}: HeaterChartProps) {
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
      series: [
        {},
        {
          label: "Heater Power",
          scale: "%",
          value: (_u: uPlot, v: number) =>
            v == null ? "" : v.toFixed(0) + "%",
          stroke: theme.series.heater.power,
          fill: theme.series.heater.power + "20",
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
          side: 3,
          scale: "%",
          values: (_u: uPlot, vals: number[]) =>
            vals.map((v) => v.toFixed(0) + "%"),
          stroke: theme.axisColor,
          grid: { stroke: theme.gridColor },
        },
      ],
      scales: {
        "%": {
          auto: false,
          range: yRange,
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
    [title, yRange]
  );

  // Data update logic
  const updatePlotData = useCallback(
    (plot: uPlot, newData: number[][]) => {
      try {
        plot.setData(newData as uPlot.AlignedData);
        plot.setScale("%", { min: yRange[0], max: yRange[1] });
      } catch (error) {
        if (onError)
          onError(error instanceof Error ? error : new Error(String(error)));
      }
    },
    [yRange, onError]
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
      ariaLabel={`${title} chart showing heater power over time`}
    />
  );
}

export default HeaterChart;
