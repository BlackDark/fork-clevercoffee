import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { useDarkMode, getChartTheme } from "@/hooks/use-dark-mode";

interface TemperatureChartProps {
  data: number[][];
  width?: number;
  height?: number;
  title?: string;
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
}: TemperatureChartProps) {
  const chartRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const [chartSize, setChartSize] = useState({ width: 800, height });
  const isDark = useDarkMode();

  // Memoize theme to prevent unnecessary chart recreations
  const theme = useMemo(() => getChartTheme(isDark), [isDark]);

  // Update chart size based on container width
  useEffect(() => {
    const updateSize = () => {
      if (chartRef.current) {
        const containerWidth = chartRef.current.offsetWidth - 32; // Account for padding
        setChartSize({
          width: width || Math.max(containerWidth, 300), // Minimum width of 300px
          height,
        });
      }
    };

    updateSize();

    // Add resize observer for better responsiveness
    const resizeObserver = new ResizeObserver(updateSize);
    if (chartRef.current) {
      resizeObserver.observe(chartRef.current);
    }

    return () => {
      resizeObserver.disconnect();
    };
  }, [width, height]);

  // Create chart configuration
  const createChartConfig = useCallback(() => {
    return {
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
          auto: true,
        },
        x: {
          auto: true,
        },
      },
    };
  }, [chartSize, title, theme]);

  // Main effect that handles chart creation and updates
  useEffect(() => {
    if (!chartRef.current || !data || data.length === 0 || !data[0] || data[0].length === 0) {
      return;
    }

    // Clean up existing chart
    if (plotRef.current) {
      plotRef.current.destroy();
      plotRef.current = null;
    }

    try {
      // Create new chart
      const config = createChartConfig();
      plotRef.current = new uPlot(config, data as uPlot.AlignedData, chartRef.current);
    } catch (error) {
      console.error("Error creating temperature chart:", error);
    }

    // Cleanup function
    return () => {
      if (plotRef.current) {
        plotRef.current.destroy();
        plotRef.current = null;
      }
    };
  }, [data, createChartConfig]);

  // Update chart size when chartSize changes
  useEffect(() => {
    if (plotRef.current) {
      plotRef.current.setSize({
        width: chartSize.width,
        height: chartSize.height,
      });
    }
  }, [chartSize]);

  return (
    <div
      ref={chartRef}
      className="w-full bg-background rounded-lg border p-4"
      style={{ minHeight: chartSize.height }}
    />
  );
}
