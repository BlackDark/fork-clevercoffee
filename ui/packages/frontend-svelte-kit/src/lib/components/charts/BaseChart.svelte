<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import uPlot from 'uplot';
	import type { Options as UPlotOptions } from 'uplot';
	import 'uplot/dist/uPlot.min.css';
	import { debounce } from '$lib/utils';
	import { get } from 'svelte/store';
	import { getChartTheme, getIsDarkMode } from '$lib/stores/theme-store.svelte';

	interface Props {
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

	let {
		data,
		width = undefined,
		height = 400,
		title = undefined,
		onError = undefined,
		createChartConfig,
		updatePlotData = undefined,
		minHeight = undefined,
		ariaLabel = undefined
	}: Props = $props();

	let chartRef: HTMLDivElement | null = $state(null);
	let plotRef: uPlot | null = null;
	let chartSize = $state<{ width: number; height: number }>({ width: 800, height });
	let hasError = $state(false);

	const isDark = getIsDarkMode();
	const theme = $derived(getChartTheme(isDark));

	const debouncedUpdateSize = debounce(() => {
		if (chartRef) {
			const containerWidth = chartRef.offsetWidth - 32;
			chartSize = {
				width: width || Math.max(containerWidth, 300),
				height
			};
		}
	}, 16);

	onMount(() => {
		debouncedUpdateSize();
		const resizeObserver = new ResizeObserver(debouncedUpdateSize);
		if (chartRef) {
			resizeObserver.observe(chartRef);
		}
		return () => {
			resizeObserver.disconnect();
		};
	});

	function handleError(error: Error) {
		hasError = true;
		if (onError) onError(error);
	}

	// Data update effect
	$effect(() => {
		if (!chartRef || !data || data.length === 0 || !data[0] || data[0].length === 0) {
			return;
		}
		hasError = false;
		try {
			if (!plotRef) {
				// Initialize chart
				const config = createChartConfig({ chartSize, theme, isDark: isDark });
				plotRef = new uPlot(config, data as uPlot.AlignedData, chartRef);
			} else if (updatePlotData) {
				updatePlotData(plotRef, data);
			} else {
				plotRef.setData(data as uPlot.AlignedData);
			}
		} catch (error) {
			handleError(error instanceof Error ? error : new Error(String(error)));
		}
	});

	// Theme change effect - recreate chart when theme changes
	$effect(() => {
		// Only recreate if plotRef exists and dependencies have changed
		if (!plotRef || hasError || !data || data.length === 0 || !chartRef) return;

		try {
			plotRef.destroy();
			plotRef = null;
			const config = createChartConfig({ chartSize, theme, isDark: isDark });
			plotRef = new uPlot(config, data as uPlot.AlignedData, chartRef);
		} catch (error) {
			handleError(error instanceof Error ? error : new Error(String(error)));
		}
	});

	// Size change effect
	$effect(() => {
		if (plotRef && !hasError) {
			// Track chartSize dependency
			chartSize;

			try {
				plotRef.setSize({
					width: chartSize.width,
					height: chartSize.height
				});
			} catch (error) {
				handleError(error instanceof Error ? error : new Error(String(error)));
			}
		}
	});

	onDestroy(() => {
		if (plotRef) {
			plotRef.destroy();
			plotRef = null;
		}
	});
</script>

{#if hasError}
	<div
		class="bg-background flex w-full items-center justify-center rounded-lg border p-4"
		style="min-height: {minHeight || chartSize.height}px;"
		role="alert"
	>
		<div class="text-destructive text-center">
			<p class="font-semibold">Failed to load chart</p>
			<p class="text-muted-foreground text-sm">Please try refreshing the data</p>
		</div>
	</div>
{:else}
	<div class="w-full">
		<div
			bind:this={chartRef}
			class="bg-background w-full rounded-lg border p-4"
			style="min-height: {minHeight || chartSize.height}px;"
			role="img"
			aria-label={ariaLabel || title || 'chart'}
		></div>
	</div>
{/if}
