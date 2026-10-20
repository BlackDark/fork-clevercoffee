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
  function create() {
    const stroke = dark ? "#b5c4c0" : "#52655e";
    const grid = dark ? "#2d3e37" : "#e4ebe7";
    const series: uPlot.Series[] =
      kind === "temperature"
        ? [
            {},
            {
              label: "Current",
              stroke: "#139b7c",
              width: 2,
              points: { show: false },
              value: (_, v) => (v == null ? "—" : `${v.toFixed(1)} °C`),
            },
            {
              label: "Target",
              stroke: "#d4a449",
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
              stroke: "#d4a449",
              fill: "#d4a44920",
              width: 2,
              points: { show: false },
              value: (_, v) => (v == null ? "—" : `${v.toFixed(0)}%`),
            },
          ];
    return new uPlot(
      {
        width: Math.max(240, container.clientWidth),
        height: 240,
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
        if (plot && plot.width !== width) plot.setSize({ width, height: 240 });
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
    void theme;
    if (plot) {
      untrack(() => {
        plot?.destroy();
        plot = create();
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
