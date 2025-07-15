import { useCallback, useEffect, useRef, useState } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

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

  const createChart = useCallback(() => {
    if (!chartRef.current || !data || data.length === 0) return;

    // Clean up existing plot
    if (plotRef.current) {
      plotRef.current.destroy();
    }

    const opts: uPlot.Options = {
      title,
      width: chartSize.width,
      height: chartSize.height,
      tzDate: (ts: number) => new Date(ts * 1000),
      series: [
        {
          value: "{YYYY}-{MM}-{DD} {HH}:{mm}:{ss}",
        },
        {
          label: "Current Temperature",
          scale: "°C",
          value: (_u: uPlot, v: number) =>
            v == null ? "" : v.toFixed(1) + " °C",
          show: true,
          stroke: "#008080",
          fill: "#00808010",
          width: 2,
          points: { show: false },
        },
        {
          label: "Target Temperature",
          scale: "°C",
          value: (_u: uPlot, v: number) =>
            v == null ? "" : v.toFixed(1) + " °C",
          stroke: "#9932CC",
          fill: "#9932CC10",
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
        },
        {
          scale: "°C",
          values: (_u: uPlot, vals: number[]) => vals.map((v) => v + "°C"),
        },
      ],
      scales: {
        "°C": {
          auto: true,
        },
      },
    };

    plotRef.current = new uPlot(
      opts,
      data as uPlot.AlignedData,
      chartRef.current
    );
  }, [data, chartSize.width, chartSize.height, title]);

  const updateChart = useCallback((newData: number[][]) => {
    if (plotRef.current && newData.length > 0) {
      // Check if chart is zoomed
      const isZoomed =
        plotRef.current.scales.x.min != plotRef.current.data[0][0] ||
        plotRef.current.scales.x.max !=
          plotRef.current.data[0][plotRef.current.data[0].length - 1];

      if (isZoomed) {
        const xScaleMinMax = [
          plotRef.current.scales.x.min!,
          plotRef.current.scales.x.max!,
        ];
        // Add data but don't autoscale
        plotRef.current.setData(newData as uPlot.AlignedData, false);
        // Move the zoomed area one value to the right so the window stays the same
        plotRef.current.setScale("x", {
          min: xScaleMinMax[0] + 1,
          max: xScaleMinMax[1] + 1,
        });
      } else {
        // Add data and autoscale (including new data)
        plotRef.current.setData(newData as uPlot.AlignedData);
      }
    }
  }, []);

  useEffect(() => {
    createChart();

    return () => {
      if (plotRef.current) {
        plotRef.current.destroy();
      }
    };
  }, [createChart]);

  useEffect(() => {
    if (plotRef.current && data.length > 0) {
      updateChart(data);
    }
  }, [data, updateChart]);

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
