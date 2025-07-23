<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import uPlot from 'uplot';
  import 'uplot/dist/uPlot.min.css';
  import { getChartTheme } from '$lib/hooks/use-dark-mode';
  import { debounce } from '$lib/utils';
  import { isDarkMode } from '$lib/stores/theme-store';

  const { data = [], width = undefined, height = 400, title = "Heater Power History", yRange = [0, 105] } = $props();

  let chartDiv: HTMLDivElement = $state();
  let plot: uPlot | null = null;
  let chartSize = $state({ width: 800, height });
  let hasError = $state(false);
  let isZoomed = $state(false);

  // Store zoom bounds for proper restoration
  let zoomBounds: { min: number; max: number } | null = null;
  let lastDataLength = 0;

  // Date formatting options
  const tzdateOptions: Intl.DateTimeFormatOptions = {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  };

  $effect(() => {
    console.log(data);
  })

  // Get theme based on dark mode
  $effect(() => {
    if (plot) {
      // Recreate the chart when theme changes
      const theme = getChartTheme($isDarkMode);
      recreateChart();
    }
  });

  // Debounced resize handler
  const debouncedUpdateSize = debounce(() => {
    if (chartDiv) {
      const containerWidth = chartDiv.offsetWidth - 32;
      chartSize = {
        width: width || Math.max(containerWidth, 300),
        height,
      };
    }
  }, 16);

  // Check if chart is zoomed
  function checkZoomState(plot: uPlot): boolean {
    if (!plot || !plot.data || !plot.data[0] || plot.data[0].length === 0) {
      return false;
    }

    const xScale = plot.scales.x;

    if (!xScale || xScale.min == null || xScale.max == null) {
      return false;
    }

    const dataStart = plot.data[0][0];
    const dataEnd = plot.data[0][plot.data[0].length - 1];

    // Check if we're showing less than the full data range
    const tolerance = 0.01; // Small tolerance for floating point comparison
    const isZoomed =
      Math.abs(xScale.min - dataStart) > tolerance ||
      Math.abs(xScale.max - dataEnd) > tolerance;
    return isZoomed;
  }

  // Update plot data with proper zoom handling
  function updatePlotData(newData: number[][]) {
    if (!plot || !newData || newData.length === 0 || !newData[0]) return;

    const wasZoomed = checkZoomState(plot);

    try {
      if (wasZoomed && zoomBounds) {
        // Calculate how much data has been added
        const currentDataLength = newData[0].length;
        const dataAdded = currentDataLength - lastDataLength;

        // Shift zoom window by the amount of new data points
        const newMin = zoomBounds.min + dataAdded;
        const newMax = zoomBounds.max + dataAdded;

        // Update data first
        plot.setData(newData as uPlot.AlignedData, false);

        // Then restore zoom with shifted bounds
        plot.setScale("x", { min: newMin, max: newMax });

        // Update stored bounds
        zoomBounds = { min: newMin, max: newMax };
      } else {
        // Normal update - no zoom preservation needed
        plot.setData(newData as uPlot.AlignedData);
        zoomBounds = null;
        isZoomed = false;
      }

      // Always ensure Y-axis is fixed
      plot.setScale("%", { min: yRange[0], max: yRange[1] });

      // Update data length tracking
      lastDataLength = newData[0].length;
    } catch (error) {
      console.error("Error updating plot data:", error);
      hasError = true;
    }
  }

  // Create chart configuration
  function createChartConfig(): uPlot.Options {
    const theme = getChartTheme($isDarkMode);

    return {
      title,
      width: chartSize.width,
      height: chartSize.height,
      tzDate: (ts: number) => new Date(ts * 1000),
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
                zoomBounds = {
                  min: u.scales.x.min,
                  max: u.scales.x.max,
                };
              } else {
                // Clear zoom bounds when not zoomed
                zoomBounds = null;
              }

              // Update zoom state
              isZoomed = currentlyZoomed;
            }

            // Prevent Y-axis from changing
            if (key === "%" && u.scales["%"].min !== yRange[0]) {
              u.setScale("%", { min: yRange[0], max: yRange[1] });
            }
          },
        ],
      },
    };
  }

  // Initialize or recreate chart
  function initChart() {
    if (!chartDiv || !data || data.length === 0 || !data[0] || data[0].length === 0) {
      return;
    }

    hasError = false;

    try {
      if (plot) {
        plot.destroy();
        plot = null;
      }

      const config = createChartConfig();
      plot = new uPlot(config, data as uPlot.AlignedData, chartDiv);
      lastDataLength = data[0].length;
    } catch (error) {
      console.error("Error initializing heater chart:", error);
      hasError = true;
    }
  }

  // Recreate chart (for theme changes)
  function recreateChart() {
    if (!plot || hasError || !data || data.length === 0) return;

    try {
      const savedZoomBounds = zoomBounds;
      const wasZoomed = isZoomed;

      plot.destroy();
      plot = null;

      const config = createChartConfig();
      plot = new uPlot(config, data as uPlot.AlignedData, chartDiv);

      // Restore zoom state
      if (wasZoomed && savedZoomBounds && plot) {
        plot.setScale("x", {
          min: savedZoomBounds.min,
          max: savedZoomBounds.max,
        });
        zoomBounds = savedZoomBounds;
      }

      lastDataLength = data[0].length;
    } catch (error) {
      console.error("Error recreating chart:", error);
      hasError = true;
    }
  }

  // Reset zoom function
  function resetZoom() {
    if (plot) {
      plot.setScale("x", { min: 0, max: 0 });
      zoomBounds = null;
      isZoomed = false;
    }
  }

  // Update chart when data changes
  $effect(() => {
    if (plot && data && data.length > 0 && data[0] && data[0].length > 0) {
      updatePlotData(data);
    } else if (!plot && data && data.length > 0 && data[0] && data[0].length > 0) {
      initChart();
    }
  });

  // Update chart size when container size changes
  $effect(() => {
    if (plot && !hasError) {
      try {
        plot.setSize({
          width: chartSize.width,
          height: chartSize.height,
        });
      } catch (error) {
        console.error("Error updating chart size:", error);
        hasError = true;
      }
    }
  });

  // Setup resize observer
  onMount(() => {
    debouncedUpdateSize();

    const resizeObserver = new ResizeObserver(debouncedUpdateSize);
    if (chartDiv) {
      resizeObserver.observe(chartDiv);
    }

    return () => {
      resizeObserver.disconnect();
    };
  });

  // Cleanup
  onDestroy(() => {
    if (plot) {
      plot.destroy();
      plot = null;
    }
  });
</script>

<div class="w-full">
  {#if isZoomed}
    <div class="mb-2 flex items-center gap-2 text-sm text-muted-foreground">
      <span>Zoomed view active</span>
      <button
        onclick={resetZoom}
        class="px-2 py-1 text-xs bg-secondary rounded hover:bg-secondary/80 transition-colors"
      >
        Reset Zoom
      </button>
    </div>
  {/if}

  {#if hasError}
    <div
      class="w-full bg-background rounded-lg border p-4 flex items-center justify-center"
      style="min-height: {chartSize.height}px"
      role="alert"
    >
      <div class="text-destructive text-center">
        <p class="font-semibold">Failed to load chart</p>
        <p class="text-sm text-muted-foreground">
          Please try refreshing the data
        </p>
      </div>
    </div>
  {:else}
    <div
      bind:this={chartDiv}
      class="w-full bg-background rounded-lg border p-4"
      style="min-height: {chartSize.height}px"
      role="img"
      aria-label="{title} chart showing heater power over time"
    ></div>
  {/if}
</div>
