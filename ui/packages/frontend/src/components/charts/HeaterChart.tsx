import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { useDarkMode, getChartTheme } from "@/hooks/use-dark-mode";
import { debounce } from "@/lib/utils";

interface HeaterChartProps {
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
  const chartRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const [chartSize, setChartSize] = useState({ width: 800, height });
  const [hasError, setHasError] = useState(false);
  const [isZoomed, setIsZoomed] = useState(false);

  // Store zoom bounds for proper restoration
  const zoomBounds = useRef<{ min: number; max: number } | null>(null);
  const lastDataLength = useRef<number>(0);

  const isDark = useDarkMode();
  const theme = useMemo(() => getChartTheme(isDark), [isDark]);

  const debouncedUpdateSize = useMemo(
    () =>
      debounce(() => {
        if (chartRef.current) {
          const containerWidth = chartRef.current.offsetWidth - 32;
          setChartSize({
            width: width || Math.max(containerWidth, 300),
            height,
          });
        }
      }, 16),
    [width, height]
  );

  useEffect(() => {
    debouncedUpdateSize();
    const resizeObserver = new ResizeObserver(debouncedUpdateSize);
    if (chartRef.current) {
      resizeObserver.observe(chartRef.current);
    }
    return () => {
      resizeObserver.disconnect();
    };
  }, [debouncedUpdateSize]);

  // Robust zoom detection
  const checkZoomState = useCallback((plot: uPlot) => {
    if (!plot || !plot.data || !plot.data[0] || plot.data[0].length === 0) {
      return false;
    }

    const xScale = plot.scales.x;

    if (xScale.min == null || xScale.max == null) {
      return false; // No zoom if scale is not set
    }

    const dataStart = plot.data[0][0];
    const dataEnd = plot.data[0][plot.data[0].length - 1];

    // Check if we're showing less than the full data range
    const tolerance = 0.01; // Small tolerance for floating point comparison
    const isZoomed =
      Math.abs(xScale.min - dataStart) > tolerance ||
      Math.abs(xScale.max - dataEnd) > tolerance;

    return isZoomed;
  }, []);

  const updatePlotData = useCallback(
    (newData: number[][]) => {
      if (!plotRef.current || !newData || newData.length === 0 || !newData[0])
        return;

      const plot = plotRef.current;
      const wasZoomed = checkZoomState(plot);

      try {
        if (wasZoomed && zoomBounds.current) {
          // Calculate how much data has been added
          const currentDataLength = newData[0].length;
          const dataAdded = currentDataLength - lastDataLength.current;

          // Shift zoom window by the amount of new data points
          const newMin = zoomBounds.current.min + dataAdded;
          const newMax = zoomBounds.current.max + dataAdded;

          // Update data first
          plot.setData(newData as uPlot.AlignedData, false);

          // Then restore zoom with shifted bounds
          plot.setScale("x", { min: newMin, max: newMax });

          // Update stored bounds
          zoomBounds.current = { min: newMin, max: newMax };

          // Keep zoom state as true - don't check during update
          // The setScale hook will handle the final state update
        } else {
          // Normal update - no zoom preservation needed
          plot.setData(newData as uPlot.AlignedData);
          zoomBounds.current = null;
          setIsZoomed(false);
        }

        // Always ensure Y-axis is fixed
        plot.setScale("%", { min: yRange[0], max: yRange[1] });

        // Update data length tracking
        lastDataLength.current = newData[0].length;

        // Don't update zoom state here - let the setScale hook handle it
      } catch (error) {
        console.error("Error updating plot data:", error);
        onError?.(error instanceof Error ? error : new Error(String(error)));
      }
    },
    [checkZoomState, yRange, onError]
  );

  const createChartConfig = useCallback((): uPlot.Options => {
    return {
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
        // sync: {
        //   key: null,
        // },
      },
      hooks: {
        setScale: [
          (u: uPlot, key: string) => {
            if (key === "x") {
              const currentlyZoomed = checkZoomState(u);

              if (
                currentlyZoomed &&
                u.scales.x.min != null &&
                u.scales.x.max != null
              ) {
                // Store current zoom bounds
                zoomBounds.current = {
                  min: u.scales.x.min,
                  max: u.scales.x.max,
                };
              } else {
                // Clear zoom bounds when not zoomed
                zoomBounds.current = null;
              }

              // Only update state if it actually changed
              setIsZoomed((prevZoomed) => {
                if (prevZoomed !== currentlyZoomed) {
                  return currentlyZoomed;
                }
                return prevZoomed;
              });
            }

            // Prevent Y-axis from changing
            if (key === "%" && u.scales["%"].min !== yRange[0]) {
              u.setScale("%", { min: yRange[0], max: yRange[1] });
            }
          },
        ],
      },
    };
  }, [chartSize, title, theme, yRange, checkZoomState]);

  const handleError = useCallback(
    (error: Error) => {
      console.error("Error with heater chart:", error);
      setHasError(true);
      onError?.(error);
    },
    [onError]
  );

  // Chart initialization and updates
  useEffect(() => {
    if (
      !chartRef.current ||
      !data ||
      data.length === 0 ||
      !data[0] ||
      data[0].length === 0
    ) {
      return;
    }

    setHasError(false);

    try {
      if (!plotRef.current) {
        // Initialize chart
        const config = createChartConfig();
        plotRef.current = new uPlot(
          config,
          data as uPlot.AlignedData,
          chartRef.current
        );
        lastDataLength.current = data[0].length;
      } else {
        // Update existing chart
        updatePlotData(data);
      }
    } catch (error) {
      handleError(error instanceof Error ? error : new Error(String(error)));
    }
  }, [data, createChartConfig, updatePlotData, handleError]);

  // Handle theme changes
  useEffect(() => {
    if (!plotRef.current || hasError || !data || data.length === 0) return;

    try {
      const savedZoomBounds = zoomBounds.current;
      const wasZoomed = isZoomed;

      plotRef.current.destroy();
      plotRef.current = null;

      const config = createChartConfig();
      plotRef.current = new uPlot(
        config,
        data as uPlot.AlignedData,
        chartRef.current!
      );

      // Restore zoom state
      if (wasZoomed && savedZoomBounds && plotRef.current) {
        plotRef.current.setScale("x", {
          min: savedZoomBounds.min,
          max: savedZoomBounds.max,
        });
        zoomBounds.current = savedZoomBounds;
      }

      lastDataLength.current = data[0].length;
    } catch (error) {
      handleError(error instanceof Error ? error : new Error(String(error)));
    }
  }, [theme, yRange, createChartConfig, handleError, data, hasError, isZoomed]);

  // Update chart size
  useEffect(() => {
    if (plotRef.current && !hasError) {
      try {
        plotRef.current.setSize({
          width: chartSize.width,
          height: chartSize.height,
        });
      } catch (error) {
        handleError(error instanceof Error ? error : new Error(String(error)));
      }
    }
  }, [chartSize, hasError, handleError]);

  // Reset zoom function
  const resetZoom = useCallback(() => {
    if (plotRef.current) {
      plotRef.current.setScale("x", { min: 0, max: 0 });
      zoomBounds.current = null;
      setIsZoomed(false);
    }
  }, []);

  // Cleanup
  useEffect(() => {
    return () => {
      if (plotRef.current) {
        plotRef.current.destroy();
        plotRef.current = null;
      }
    };
  }, []);

  if (hasError) {
    return (
      <div
        className="w-full bg-background rounded-lg border p-4 flex items-center justify-center"
        style={{ minHeight: chartSize.height }}
        role="alert"
      >
        <div className="text-destructive text-center">
          <p className="font-semibold">Failed to load chart</p>
          <p className="text-sm text-muted-foreground">
            Please try refreshing the data
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="w-full">
      {isZoomed && (
        <div className="mb-2 flex items-center gap-2 text-sm text-muted-foreground">
          <span>Zoomed view active</span>
          <button
            onClick={resetZoom}
            className="px-2 py-1 text-xs bg-secondary rounded hover:bg-secondary/80 transition-colors"
          >
            Reset Zoom
          </button>
        </div>
      )}

      <div
        ref={chartRef}
        className="w-full bg-background rounded-lg border p-4"
        style={{ minHeight: chartSize.height }}
        role="img"
        aria-label={`${title} chart showing heater power over time`}
      />
    </div>
  );
}

export default HeaterChart;
