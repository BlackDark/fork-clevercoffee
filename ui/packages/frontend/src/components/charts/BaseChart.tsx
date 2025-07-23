import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import uPlot from "uplot";
import type { Options as UPlotOptions } from "uplot";
import "uplot/dist/uPlot.min.css";
import { useDarkMode, getChartTheme } from "@/hooks/use-dark-mode";
import { debounce } from "@/lib/utils";

export interface BaseChartProps {
  data: number[][];
  width?: number;
  height?: number;
  title?: string;
  onError?: (error: Error) => void;
  createChartConfig: (opts: {
    chartSize: { width: number; height: number };
    theme: ReturnType<typeof getChartTheme>;
    isDark: boolean;
  }) => UPlotOptions;
  updatePlotData?: (plot: uPlot, newData: number[][]) => void;
  minHeight?: number;
  ariaLabel?: string;
}

export function BaseChart({
  data,
  width,
  height = 400,
  title,
  onError,
  createChartConfig,
  updatePlotData,
  minHeight,
  ariaLabel,
}: BaseChartProps) {
  const chartRef = useRef<HTMLDivElement | null>(null);
  const plotRef = useRef<uPlot | null>(null);
  const [chartSize, setChartSize] = useState<{ width: number; height: number }>(
    { width: 800, height }
  );
  const [hasError, setHasError] = useState(false);

  const isDark = useDarkMode();
  const theme = useMemo(() => getChartTheme(isDark), [isDark]);

  const debouncedUpdateSize = useMemo(
    () =>
      debounce(() => {
        if (chartRef.current) {
          const containerWidth =
            (chartRef.current as HTMLDivElement).offsetWidth - 32;
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

  const handleError = useCallback(
    (error: Error) => {
      setHasError(true);
      if (onError) onError(error);
    },
    [onError]
  );

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
        const config = createChartConfig({ chartSize, theme, isDark });
        plotRef.current = new uPlot(
          config,
          data as uPlot.AlignedData,
          chartRef.current
        );
      } else if (updatePlotData) {
        updatePlotData(plotRef.current, data);
      } else {
        plotRef.current.setData(data as uPlot.AlignedData);
      }
    } catch (error) {
      handleError(error instanceof Error ? error : new Error(String(error)));
    }
  }, [
    data,
    createChartConfig,
    updatePlotData,
    handleError,
    chartSize,
    theme,
    isDark,
  ]);

  useEffect(() => {
    if (!plotRef.current || hasError || !data || data.length === 0) return;
    try {
      plotRef.current.destroy();
      plotRef.current = null;
      const config = createChartConfig({ chartSize, theme, isDark });
      plotRef.current = new uPlot(
        config,
        data as uPlot.AlignedData,
        chartRef.current!
      );
    } catch (error) {
      handleError(error instanceof Error ? error : new Error(String(error)));
    }
  }, [
    theme,
    createChartConfig,
    handleError,
    data,
    hasError,
    chartSize,
    isDark,
  ]);

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
        style={{ minHeight: minHeight || chartSize.height }}
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
      <div
        ref={chartRef}
        className="w-full bg-background rounded-lg border p-4"
        style={{ minHeight: minHeight || chartSize.height }}
        role="img"
        aria-label={ariaLabel || title || "chart"}
      />
    </div>
  );
}
