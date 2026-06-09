import { useEffect, useMemo, useRef, useState } from "react";
import type { Options as UPlotOptions } from "uplot";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { getChartTheme, useDarkMode } from "@/hooks/use-dark-mode";
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

function reportChartError(
  onError: BaseChartProps["onError"],
  setHasError: (value: boolean) => void,
  error: unknown,
) {
  const chartError = error instanceof Error ? error : new Error(String(error));
  queueMicrotask(() => {
    setHasError(true);
    onError?.(chartError);
  });
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
    { width: 800, height },
  );
  const [hasError, setHasError] = useState(false);

  const isDark = useDarkMode();
  const theme = useMemo(() => getChartTheme(isDark), [isDark]);

  useEffect(() => {
    const updateSize = debounce(() => {
      const element = chartRef.current;
      if (!element) {
        return;
      }

      const containerWidth = element.offsetWidth - 32;
      setChartSize({
        width: width || Math.max(containerWidth, 100),
        height,
      });
    }, 16);

    updateSize();
    const element = chartRef.current;
    if (!element) {
      return;
    }

    const resizeObserver = new ResizeObserver(updateSize);
    resizeObserver.observe(element);
    return () => {
      resizeObserver.disconnect();
    };
  }, [width, height]);

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

    queueMicrotask(() => setHasError(false));

    try {
      if (!plotRef.current) {
        const config = createChartConfig({ chartSize, theme, isDark });
        plotRef.current = new uPlot(
          config,
          data as uPlot.AlignedData,
          chartRef.current,
        );
      } else if (updatePlotData) {
        updatePlotData(plotRef.current, data);
      } else {
        plotRef.current.setData(data as uPlot.AlignedData);
      }
    } catch (error) {
      reportChartError(onError, setHasError, error);
    }
  }, [
    data,
    createChartConfig,
    updatePlotData,
    onError,
    chartSize,
    theme,
    isDark,
  ]);

  useEffect(() => {
    if (!plotRef.current || hasError || !data || data.length === 0) return;
    const container = chartRef.current;
    if (!container) return;
    try {
      plotRef.current.destroy();
      plotRef.current = null;
      const config = createChartConfig({ chartSize, theme, isDark });
      plotRef.current = new uPlot(config, data as uPlot.AlignedData, container);
    } catch (error) {
      reportChartError(onError, setHasError, error);
    }
  }, [theme, createChartConfig, onError, data, hasError, chartSize, isDark]);

  useEffect(() => {
    if (plotRef.current && !hasError) {
      try {
        plotRef.current.setSize({
          width: chartSize.width,
          height: chartSize.height,
        });
      } catch (error) {
        reportChartError(onError, setHasError, error);
      }
    }
  }, [chartSize, hasError, onError]);

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
