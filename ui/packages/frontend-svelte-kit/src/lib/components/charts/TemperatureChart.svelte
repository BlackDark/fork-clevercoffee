<script lang="ts">
	import uPlot from 'uplot';
	import BaseChart from '$lib/components/charts/BaseChart.svelte';
	import type { getChartTheme } from '$lib/stores/theme-store.svelte';

	export let data: number[][];
	export let width: number | undefined = undefined;
	export let height: number = 400;
	export let title: string = 'Temperature History';
	export let tempRange: [number, number] | undefined = undefined;
	export let autoScaleY: boolean = true;
	export let onError: ((error: Error) => void) | undefined = undefined;

	const tzdateOptions: Intl.DateTimeFormatOptions = {
		hour: '2-digit',
		minute: '2-digit',
		second: '2-digit'
	};

	// Chart config
	const createChartConfig = ({
		chartSize,
		theme
	}: {
		chartSize: { width: number; height: number };
		theme: ReturnType<typeof getChartTheme>;
	}) => ({
		title,
		width: chartSize.width,
		height: chartSize.height,
		tzDate: (ts: number) => new Date(ts * 1000),
		series: [
			{},
			{
				label: 'Current Temperature',
				scale: '°C',
				value: (_u: uPlot, v: number) => (v == null ? '' : v.toFixed(1) + ' °C'),
				show: true,
				stroke: theme.series.temperature.current,
				fill: theme.series.temperature.current + '20',
				width: 2,
				points: { show: false }
			},
			{
				label: 'Target Temperature',
				scale: '°C',
				value: (_u: uPlot, v: number) => (v == null ? '' : v.toFixed(1) + ' °C'),
				stroke: theme.series.temperature.target,
				fill: theme.series.temperature.target + '20',
				width: 2,
				show: true,
				points: { show: false }
			}
		],
		axes: [
			{
				values: (_u: uPlot, vals: number[]) =>
					vals.map((v) => new Date(v * 1000).toLocaleString('de-DE', tzdateOptions)),
				stroke: theme.axisColor,
				grid: { stroke: theme.gridColor }
			},
			{
				scale: '°C',
				values: (_u: uPlot, vals: number[]) => vals.map((v) => v + '°C'),
				stroke: theme.axisColor,
				grid: { stroke: theme.gridColor }
			}
		],
		scales: {
			'°C': {
				auto: autoScaleY,
				...(autoScaleY === false && tempRange ? { range: tempRange } : {})
			},
			x: {
				auto: true
			}
		},
		cursor: {
			drag: {
				x: true,
				y: false
			}
		}
	});

	// Data update logic
	const updatePlotData = (plot: uPlot, newData: number[][]) => {
		try {
			plot.setData(newData as uPlot.AlignedData);
			if (!autoScaleY && tempRange) {
				plot.setScale('°C', { min: tempRange[0], max: tempRange[1] });
			}
		} catch (error) {
			if (onError) onError(error instanceof Error ? error : new Error(String(error)));
		}
	};
</script>

<BaseChart
	{data}
	{width}
	{height}
	{title}
	{onError}
	{createChartConfig}
	{updatePlotData}
	minHeight={height}
	ariaLabel={`${title} chart showing temperature over time`}
/>
