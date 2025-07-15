import { useEffect, useRef, useCallback, useState } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

interface HeaterChartProps {
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

export function HeaterChart({
  data,
  width,
  height = 400,
  title = "Heater Power History",
}: HeaterChartProps) {
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
    if (!chartRef.current || data.length === 0) return;

    // Destroy existing plot
    if (plotRef.current) {
      plotRef.current.destroy();
    }

    const opts: uPlot.Options = {
      title,
      width: chartSize.width,
      height: chartSize.height,
      series: [
        {},
        {
          label: "Heater Power",
          scale: "%",
          value: (_u, v) => (v == null ? "" : (v as number).toFixed(0) + "%"),
          stroke: "#778899",
          fill: "#77889910",
          width: 2,
          show: true,
          points: { show: false },
        },
      ],
      axes: [
        {
          values: (_u, vals) =>
            vals.map((v) =>
              new Date(v * 1000).toLocaleString("de-DE", tzdateOptions)
            ),
        },
        {
          side: 3,
          scale: "%",
          values: (_u, vals) =>
            vals.map((v) => +(v as number).toFixed(0) + "%"),
        },
      ],
      scales: {
        "%": {
          auto: false,
          range: [0, 105],
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
      plotRef.current.setData(newData as uPlot.AlignedData);
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
