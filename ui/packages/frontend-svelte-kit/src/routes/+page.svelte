<script lang="ts">
	import { onMount } from 'svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Switch } from '$lib/components/ui/switch';
	import { Alert, AlertDescription, AlertTitle } from '$lib/components/ui/alert';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { Popover, PopoverContent, PopoverTrigger } from '$lib/components/ui/popover';
	import {
		HelpCircle,
		Loader2,
		AlertCircle,
		RefreshCw,
		Thermometer,
		Zap,
		Settings,
		TrendingUp,
		Activity
	} from 'lucide-svelte';
	import parameterLabels from '$lib/parameter-labels';
	import { parameterHelpTexts } from '$lib/parameter-help-texts';

	// Lazy-loaded chart components
	import TemperatureChart from '$lib/components/charts/TemperatureChart.svelte';
	import HeaterChart from '$lib/components/charts/HeaterChart.svelte';
	import {
		cleverCoffeeState,
		fetchTemperatureAndChartData,
		fetchHistoryData,
		updateParameter,
		saveParameters,
		retryConnection,
		togglePid,
		toggleSteam,
		toggleBackflush,
		toggleTareScale,
		toggleScaleCalibration
	} from '$lib/stores/clever-coffee-store.svelte';
	import { toast } from 'svelte-sonner';

	// Access state properties directly - these are reactive
	const parameters = $derived(cleverCoffeeState.parameters);
	const currentTempData = $derived(cleverCoffeeState.currentTempData);
	const loadingParams = $derived(cleverCoffeeState.loadingParams);
	const isLoadingTemp = $derived(cleverCoffeeState.isLoadingTemp);
	const temperatureError = $derived(cleverCoffeeState.temperatureError);
	const chartError = $derived(cleverCoffeeState.chartError);
	const tempData = $derived(cleverCoffeeState.tempData);
	const heaterData = $derived(cleverCoffeeState.heaterData);
	// Derived reactive values (equivalent to useMemo)
	const brewSetpointParam = $derived(parameters.find((p) => p.name === 'brew.setpoint'));

	const functionToggleParams = $derived(
		parameters.filter((p) => ['pid.enabled', 'STEAM_MODE', 'BACKFLUSH_ON'].includes(p.name))
	);

	const scaleActionParams = $derived(
		parameters.filter((p) => ['TARE_ON', 'CALIBRATION_ON'].includes(p.name))
	);

	const scaleEnabled = $derived(
		parameters.find((p) => p.name === 'hardware.sensors.scale.enabled')?.value === 1
	);

	// Chart data derivations
	const tempChartData = $derived.by(() => {
		if (tempData.tempDates.length === 0) return [];
		return [
			tempData.tempDates.map((d: Date) => d.getTime() / 1000),
			tempData.curTempVals,
			tempData.targetTempVals
		];
	});

	const heaterChartData = $derived.by(() => {
		if (heaterData.heaterDates.length === 0) return [];
		return [
			heaterData.heaterDates.map((d: Date) => d.getTime() / 1000),
			heaterData.heaterPowerVals
		];
	});

	// Mount effect (equivalent to useEffect)
	onMount(() => {
		if (fetchHistoryData) {
			fetchHistoryData();
		}
	});

	// Helper function to get the correct step value for number inputs
	function getNumberStep(param: { type: number }) {
		switch (param.type) {
			case 0: // integer
			case 1: // uint8
				return 1;
			case 2: // double
			case 3: // float
				return 0.01;
			default:
				return 1;
		}
	}

	// Handle form submission for brew setpoint
	async function handleSubmitParameters(e: SubmitEvent) {
		e.preventDefault();
		const success = await saveParameters(); // Only submit brew.setpoint from home page

		if (success) {
			toast.success('Parameters saved successfully', {
				description: 'Your configuration has been updated.'
			});
		} else {
			toast.error('Failed to save parameters', {
				description: 'Please check your connection and try again.'
			});
		}
	}

	// Handle function toggles by calling dedicated context methods if available
	async function handleToggleFunction(paramName: string) {
		let success = false;
		if (paramName === 'pid.enabled') {
			success = await togglePid();
		} else if (paramName === 'STEAM_MODE') {
			success = await toggleSteam();
		} else if (paramName === 'BACKFLUSH_ON') {
			success = await toggleBackflush();
		} else {
			// Fallback: update and save parameter
			const param = parameters.find((p) => p.name === paramName);
			if (!param) return;
			const newValue = param.value === 1 ? 0 : 1;
			updateParameter(paramName, newValue);
			success = await saveParameters();
		}
		if (success) {
			toast.success(`${parameterLabels.en[paramName] || paramName} toggled successfully`, {
				description: 'Setting updated via API endpoint or parameter save.'
			});
		} else {
			toast.error('Failed to toggle', {
				description: 'Please check your connection and try again.'
			});
		}
	}

	// Handle scale actions by calling dedicated context methods
	async function handleScaleAction(paramName: string) {
		let success = false;
		if (paramName === 'TARE_ON') {
			success = await toggleTareScale();
		} else if (paramName === 'CALIBRATION_ON') {
			success = await toggleScaleCalibration();
		} else {
			updateParameter(paramName, 1);
			success = true;
		}
		if (success) {
			toast.success(`${parameterLabels.en[paramName] || paramName} action triggered`, {
				description: 'Command executed successfully'
			});
		} else {
			toast.error('Failed to trigger action', {
				description: 'Please check your connection and try again.'
			});
		}
	}

	// Handle temperature retry with feedback
	async function handleTemperatureRetry() {
		const success = await fetchTemperatureAndChartData(true); // Show loading during manual retry

		if (success) {
			toast.success('Temperature restored', {
				description: 'Successfully reconnected to temperature sensor'
			});
		} else {
			toast.error('Temperature Error', {
				description: 'Unable to fetch current temperature from sensor'
			});
		}
	}

	// Handle chart data retry with feedback
	async function handleChartRetry() {
		const success = await fetchHistoryData();

		if (!success) {
			toast.error('Chart Error', {
				description: 'Failed to load historical chart data'
			});
		}
	}
</script>

<div class="container mx-auto max-w-7xl space-y-6 p-6">
	<!-- Connection Error Alert -->
	{#if temperatureError}
		<Alert
			class="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive"
		>
			<AlertCircle class="h-4 w-4" />
			<AlertDescription>
				{temperatureError}
				<Button variant="outline" size="sm" class="ml-3" onclick={retryConnection}>
					<RefreshCw class="mr-1 h-4 w-4" />
					Retry
				</Button>
			</AlertDescription>
		</Alert>
	{/if}

	<!-- Machine Status and Functions Grid -->
	<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
		<!-- Machine Status Card -->
		<Card>
			<CardHeader>
				<CardTitle class="flex items-center gap-2">
					<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-500/10">
						<Activity class="h-5 w-5 text-blue-600" />
					</div>
					Machine Status
				</CardTitle>
			</CardHeader>
			<CardContent>
				<div class="space-y-4">
					<div class="grid grid-cols-1 gap-4 md:grid-cols-2">
						<!-- Temperature Display -->
						<div class="space-y-3">
							<div class="flex items-center gap-2 font-medium">
								<Thermometer class="h-4 w-4 text-red-500" />
								Current Temperature
							</div>
							{#if isLoadingTemp}
								<div class="flex items-center gap-3">
									<Skeleton class="h-10 w-20" />
									<div class="space-y-1">
										<Skeleton class="h-3 w-16" />
										<Skeleton class="h-2 w-12" />
									</div>
								</div>
							{:else if temperatureError}
								<div class="space-y-3">
									<div class="flex items-center gap-2">
										<AlertCircle class="text-destructive h-5 w-5" />
										<div>
											<p class="text-destructive font-semibold">
												{temperatureError.includes('offline')
													? 'Sensor Offline'
													: temperatureError.includes('retrying')
														? 'Retrying...'
														: 'Connection Failed'}
											</p>
											<p class="text-muted-foreground text-xs">
												{temperatureError}
											</p>
										</div>
									</div>
									<Button
										variant="outline"
										size="sm"
										onclick={handleTemperatureRetry}
										disabled={temperatureError.includes('retrying')}
									>
										<RefreshCw
											class={`mr-2 h-4 w-4 ${
												temperatureError.includes('retrying') ? 'animate-spin' : ''
											}`}
										/>
										{temperatureError.includes('retrying') ? 'Retrying...' : 'Retry'}
									</Button>
								</div>
							{:else if currentTempData}
								<div class="space-y-1">
									<div class="flex items-baseline gap-2">
										<span class="text-3xl font-bold tracking-tight">
											{currentTempData.currentTemp.toFixed(2)}
										</span>
										<span class="text-muted-foreground text-xl"> °C </span>
									</div>
									<div class="flex items-center gap-1 text-xs text-green-600">
										<div class="h-2 w-2 animate-pulse rounded-full bg-green-500"></div>
										Live
									</div>
								</div>
							{:else}
								<div class="space-y-3">
									<div class="flex items-center gap-2">
										<AlertCircle class="h-5 w-5 text-orange-500" />
										<div>
											<p class="font-semibold text-orange-600">No Data</p>
											<p class="text-muted-foreground text-xs">Temperature sensor offline</p>
										</div>
									</div>
									<Button variant="outline" size="sm" onclick={handleTemperatureRetry}>
										<RefreshCw class="mr-2 h-4 w-4" />
										Retry
									</Button>
								</div>
							{/if}
						</div>

						<!-- Heater Power Display -->
						<div class="space-y-3">
							<div class="flex items-center gap-2 font-medium">
								<Zap class="h-4 w-4 text-yellow-500" />
								Current Heater Power
							</div>
							{#if isLoadingTemp}
								<div class="flex items-center gap-3">
									<Skeleton class="h-10 w-20" />
									<div class="space-y-1">
										<Skeleton class="h-3 w-16" />
										<Skeleton class="h-2 w-12" />
									</div>
								</div>
							{:else if temperatureError}
								<div class="space-y-3">
									<div class="flex items-center gap-2">
										<AlertCircle class="text-destructive h-5 w-5" />
										<div>
											<p class="text-destructive font-semibold">
												{temperatureError.includes('offline')
													? 'Sensor Offline'
													: temperatureError.includes('retrying')
														? 'Retrying...'
														: 'Connection Failed'}
											</p>
											<p class="text-muted-foreground text-xs">
												{temperatureError}
											</p>
										</div>
									</div>
									<Button
										variant="outline"
										size="sm"
										onclick={handleTemperatureRetry}
										disabled={temperatureError.includes('retrying')}
									>
										<RefreshCw
											class={`mr-2 h-4 w-4 ${
												temperatureError.includes('retrying') ? 'animate-spin' : ''
											}`}
										/>
										{temperatureError.includes('retrying') ? 'Retrying...' : 'Retry'}
									</Button>
								</div>
							{:else if currentTempData}
								<div class="space-y-1">
									<div class="flex items-baseline gap-2">
										<span class="text-3xl font-bold tracking-tight">
											{currentTempData.heaterPower.toFixed(2) || '0.00'}
										</span>
										<span class="text-muted-foreground text-xl">%</span>
									</div>
									<div class="flex items-center gap-1 text-xs text-green-600">
										<div class="h-2 w-2 animate-pulse rounded-full bg-green-500"></div>
										Live
									</div>
								</div>
							{:else}
								<div class="space-y-1">
									<div class="flex items-baseline gap-2">
										<span class="text-3xl font-bold tracking-tight"> 0.0 </span>
										<span class="text-muted-foreground text-xl">%</span>
									</div>
									<div class="text-muted-foreground flex items-center gap-1 text-xs">
										<div class="h-2 w-2 rounded-full bg-gray-400"></div>
										Offline
									</div>
								</div>
							{/if}
						</div>
					</div>

					<!-- Temperature Setpoint Control -->
					{#if loadingParams}
						<div class="flex items-center justify-between rounded-lg border p-4">
							<div class="space-y-1">
								<Skeleton class="h-4 w-32" />
								<Skeleton class="h-3 w-24" />
							</div>
							<Skeleton class="h-10 w-20" />
						</div>
					{:else if brewSetpointParam}
						<form onsubmit={handleSubmitParameters}>
							<div
								class="hover:bg-accent/50 flex items-center justify-between rounded-lg border p-4 transition-colors"
							>
								<div class="space-y-1">
									<div class="flex items-center gap-2 font-medium">
										<Settings class="h-4 w-4 text-green-600" />
										{parameterLabels.en[brewSetpointParam.name] || brewSetpointParam.name}
										{#if parameterHelpTexts[brewSetpointParam.name]}
											<Popover>
												<PopoverTrigger>
													<Button variant="ghost" size="icon" class="ml-1 h-6 w-6" tabindex={0}>
														<HelpCircle class="text-muted-foreground h-4 w-4" />
													</Button>
												</PopoverTrigger>
												<PopoverContent class="w-80 text-xs">
													<span>
														{@html parameterHelpTexts[brewSetpointParam.name] ||
															'Loading help text...'}
													</span>
												</PopoverContent>
											</Popover>
										{/if}
									</div>
									<div class="text-muted-foreground text-sm">
										Target: {brewSetpointParam.value}°C
									</div>
								</div>
								<div class="flex items-center gap-2">
									<Input
										id={brewSetpointParam.name}
										type="number"
										step={getNumberStep(brewSetpointParam)}
										value={brewSetpointParam.value}
										oninput={(e: any) => updateParameter(brewSetpointParam.name, e.target.value)}
										min={brewSetpointParam.min}
										max={brewSetpointParam.max}
										class="w-20"
									/>
									<Button type="submit" size="sm" disabled={isLoadingTemp}>
										{#if isLoadingTemp}
											<Loader2 class="h-4 w-4 animate-spin" />
										{:else}
											Save
										{/if}
									</Button>
								</div>
							</div>
						</form>
					{/if}
				</div>
			</CardContent>
		</Card>

		<!-- Machine Functions Card -->
		<Card>
			<CardHeader>
				<CardTitle class="flex items-center gap-2">
					<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-purple-500/10">
						<Zap class="h-5 w-5 text-purple-600" />
					</div>
					Machine Functions
				</CardTitle>
			</CardHeader>
			<CardContent>
				{#if loadingParams}
					<div class="space-y-4">
						{#each [1, 2, 3] as i (i)}
							<div class="flex items-center justify-between rounded-lg border p-4">
								<Skeleton class="h-5 w-32" />
								<Skeleton class="h-6 w-12" />
							</div>
						{/each}
					</div>
				{:else}
					<div class="space-y-4">
						<!-- Function Toggles -->
						<div class="grid gap-3">
							{#each functionToggleParams as param (param.name)}
								<div
									class="hover:bg-accent/50 flex items-center justify-between rounded-lg border p-4 transition-colors"
								>
									<div class="space-y-1">
										<div class="font-medium">
											{parameterLabels.en[param.name] || param.name}
										</div>
										<div class="text-muted-foreground text-sm">
											{param.value === 1 ? 'Enabled' : 'Disabled'}
										</div>
									</div>
									<Switch
										id={param.name}
										checked={param.value === 1}
										onCheckedChange={() => handleToggleFunction(param.name)}
									/>
								</div>
							{/each}
						</div>

						<!-- Scale Operations -->
						{#if scaleEnabled && scaleActionParams.length > 0}
							<div class="space-y-3">
								<div class="flex items-center gap-2 border-t pt-4">
									<div class="flex h-8 w-8 items-center justify-center rounded-lg bg-blue-500/10">
										<Settings class="h-4 w-4 text-blue-600" />
									</div>
									<h4 class="font-medium">Scale Operations</h4>
								</div>
								<div class="grid gap-3">
									{#each scaleActionParams as param (param.name)}
										<div class="flex items-center justify-between rounded-lg border p-4">
											<div class="space-y-1">
												<div class="font-medium">
													{parameterLabels.en[param.name] || param.name}
												</div>
												<div class="text-muted-foreground text-sm">
													{param.name === 'TARE_ON'
														? 'Reset scale to zero'
														: 'Calibrate scale accuracy'}
												</div>
											</div>
											<Button variant="outline" onclick={() => handleScaleAction(param.name)}>
												{param.name === 'TARE_ON' ? 'Tare Scale' : 'Start Calibration'}
											</Button>
										</div>
									{/each}
								</div>
							</div>
						{/if}
					</div>
				{/if}
			</CardContent>
		</Card>
	</div>

	<!-- Temperature Chart -->
	<Card>
		<CardHeader>
			<CardTitle class="flex items-center gap-2">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-red-500/10">
					<TrendingUp class="h-5 w-5 text-red-600" />
				</div>
				Temperature History
				<Popover>
					<PopoverTrigger>
						<Button variant="ghost" size="icon" class="ml-auto h-8 w-8">
							<HelpCircle class="h-4 w-4" />
						</Button>
					</PopoverTrigger>
					<PopoverContent class="w-80">
						History of the boiler temperature. Drag to zoom, double-click to reset zoom again.
					</PopoverContent>
				</Popover>
			</CardTitle>
		</CardHeader>
		<CardContent>
			{#if chartError}
				<Alert>
					<AlertCircle class="h-4 w-4" />
					<AlertTitle class="flex items-center justify-between">
						<span>{chartError}</span>
						<Button variant="outline" size="sm" onclick={handleChartRetry}>
							<RefreshCw class="mr-2 h-4 w-4" />
							Retry
						</Button>
					</AlertTitle>
				</Alert>
			{:else if tempChartData.length === 0}
				<div class="flex h-64 items-center justify-center rounded-lg border border-dashed">
					<div class="space-y-2 text-center">
						<Loader2 class="text-muted-foreground mx-auto h-8 w-8 animate-spin" />
						<p class="text-muted-foreground text-sm">Loading temperature data...</p>
					</div>
				</div>
			{:else}
				<TemperatureChart data={tempChartData} height={300} title="Temperature History" />
			{/if}
		</CardContent>
	</Card>

	<!-- Heater Chart -->
	<Card>
		<CardHeader>
			<CardTitle class="flex items-center gap-2">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-yellow-500/10">
					<Zap class="h-5 w-5 text-yellow-600" />
				</div>
				Heater Power History
				<Popover>
					<PopoverTrigger>
						<Button variant="ghost" size="icon" class="ml-auto h-8 w-8">
							<HelpCircle class="h-4 w-4" />
						</Button>
					</PopoverTrigger>
					<PopoverContent class="w-80">
						History of the controlled heater power. Drag to zoom, double-click to reset zoom again.
					</PopoverContent>
				</Popover>
			</CardTitle>
		</CardHeader>
		<CardContent>
			{#if chartError}
				<Alert>
					<AlertCircle class="h-4 w-4" />
					<AlertDescription class="flex items-center justify-between">
						<span>{chartError}</span>
						<Button variant="outline" size="sm" onclick={handleChartRetry}>
							<RefreshCw class="mr-2 h-4 w-4" />
							Retry
						</Button>
					</AlertDescription>
				</Alert>
			{:else if heaterChartData.length === 0}
				<div class="flex h-64 items-center justify-center rounded-lg border border-dashed">
					<div class="space-y-2 text-center">
						<Loader2 class="text-muted-foreground mx-auto h-8 w-8 animate-spin" />
						<p class="text-muted-foreground text-sm">Loading heater data...</p>
					</div>
				</div>
			{:else}
				<div>
					<HeaterChart data={heaterChartData} height={300} title="Heater Power History" />
				</div>
			{/if}
		</CardContent>
	</Card>
</div>
