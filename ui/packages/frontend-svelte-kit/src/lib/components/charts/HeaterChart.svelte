<script lang="ts">
	import type { getChartTheme } from '$lib/stores/theme-store.svelte';
	import BaseChart from './BaseChart.svelte';
	import uPlot from 'uplot';

	export let data: number[][];
	export let width: number | undefined = undefined;
	export let height: number = 400;
	export let title: string = 'Heater Power History';
	export let yRange: [number, number] = [0, 105];
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
		series: [
			{},
			{
				label: 'Heater Power',
				scale: '%',
				value: (_u: uPlot, v: number) => (v == null ? '' : v.toFixed(0) + '%'),
				stroke: theme.series.heater.power,
				fill: theme.series.heater.power + '20',
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
				side: 3,
				scale: '%',
				values: (_u: uPlot, vals: number[]) => vals.map((v) => v.toFixed(0) + '%'),
				stroke: theme.axisColor,
				grid: { stroke: theme.gridColor }
			}
		],
		scales: {
			'%': {
				auto: false,
				range: yRange
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
			plot.setScale('%', { min: yRange[0], max: yRange[1] });
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
	ariaLabel={`${title} chart showing heater power over time`}
/>
