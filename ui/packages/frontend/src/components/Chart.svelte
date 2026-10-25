<script lang="ts">
  import { onMount, untrack } from "svelte";
  import uPlot from "uplot";
  import "uplot/dist/uPlot.min.css";
  let {
    data,
    kind = "temperature",
    dark = false,
  }: {
    data: number[][];
    kind?: "temperature" | "heater";
    dark?: boolean;
  } = $props();
  let container: HTMLDivElement;
  let plot: uPlot | undefined;
  let error = $state("");
  let builtDark: boolean | undefined;
  function create() {
    const stroke = dark ? "#6b7280" : "#9ca3af";
    const grid = dark ? "#374151" : "#e5e7eb";
    const series: uPlot.Series[] =
      kind === "temperature"
        ? [
            {},
            {
              label: "Current",
              stroke: dark ? "#06b6d4" : "#0891b2",
              width: 2,
              points: { show: false },
              value: (_, v) => (v == null ? "—" : `${v.toFixed(1)} °C`),
            },
            {
              label: "Target",
              stroke: dark ? "#a855f7" : "#9333ea",
              width: 2,
              dash: [5, 4],
              points: { show: false },
              value: (_, v) => (v == null ? "—" : `${v.toFixed(1)} °C`),
            },
          ]
        : [
            {},
            {
              label: "Heater power",
              stroke: dark ? "#f59e0b" : "#d97706",
              fill: dark ? "#f59e0b20" : "#d9770620",
              width: 2,
              points: { show: false },
              value: (_, v) => (v == null ? "—" : `${v.toFixed(0)}%`),
            },
          ];
    const next = new uPlot(
      {
        width: Math.max(240, container.clientWidth),
        height: 320,
        series,
        axes: [
          {
            stroke,
            grid: { stroke: grid },
            space: 75,
            size: 35,
            values: (_, values) =>
              values.map((value) =>
                new Date(value * 1000).toLocaleTimeString([], {
                  hour: "2-digit",
                  minute: "2-digit",
                  hour12: false,
                }),
              ),
          },
          {
            stroke,
            grid: { stroke: grid },
            values: (_, values) =>
              values.map((v) => `${v}${kind === "heater" ? "%" : "°"}`),
          },
        ],
        scales: kind === "heater" ? { y: { range: [0, 105] } } : {},
        cursor: { drag: { x: true, y: false } },
      },
      data as uPlot.AlignedData,
      container,
    );
    builtDark = dark;
    return next;
  }
  onMount(() => {
    try {
      plot = create();
    } catch (e) {
      error = String(e);
    }
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const width = Math.max(240, container.clientWidth);
        if (plot && plot.width !== width) plot.setSize({ width, height: 320 });
      });
    });
    observer.observe(container);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
      plot?.destroy();
      plot = undefined;
    };
  });
  $effect(() => {
    const next = data;
    if (plot) {
      try {
        plot.setData(next as uPlot.AlignedData);
      } catch (e) {
        error = String(e);
      }
    }
  });
  $effect(() => {
    const theme = dark;
    if (plot && builtDark !== theme) {
      untrack(() => {
        const previous = plot;
        plot = undefined;
        try {
          previous?.destroy();
          plot = create();
          error = "";
        } catch (e) {
          error = String(e);
        }
      });
    }
  });
</script>

<div
  class="min-w-0"
  role="img"
  aria-label={kind === "temperature"
    ? "Temperature history chart"
    : "Heater power history chart"}
  bind:this={container}
></div>
{#if error}<p role="alert">Chart unavailable: {error}</p>{/if}
