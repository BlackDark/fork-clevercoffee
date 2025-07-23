<script lang="ts">
  import { onMount, onDestroy, getContext } from 'svelte';
  import {
    fetchTemperatureAndChartData,
    fetchHistoryData,
    togglePid,
    toggleSteam,
    toggleBackflush,
    toggleTareScale,
    toggleScaleCalibration,
    updateParameter,
    retryConnection,
    cleverCoffeeState,

	saveSomeParameters

  } from '$lib/stores/clever-coffee-store.svelte';
  import { Thermometer, Power, Droplets, Scale, RefreshCw, Activity, AlertCircle, Zap, Settings, TrendingUp, HelpCircle, Loader2 } from 'lucide-svelte';
  import Card from '$lib/components/ui/card/card.svelte';
  import CardHeader from '$lib/components/ui/card/card-header.svelte';
  import CardContent from '$lib/components/ui/card/card-content.svelte';
  import CardTitle from '$lib/components/ui/card/card-title.svelte';
  import Button from '$lib/components/ui/button/button.svelte';
  import Alert from '$lib/components/ui/alert/alert.svelte';
  import AlertDescription from '$lib/components/ui/alert/alert-description.svelte';
  import AlertTitle from '$lib/components/ui/alert/alert-title.svelte';
  import { Popover, PopoverContent, PopoverTrigger } from '$lib/components/ui/popover';
  import Skeleton from '$lib/components/ui/skeleton/skeleton.svelte';
  import TemperatureChart from '$lib/components/charts/TemperatureChart.svelte';
  import HeaterChart from '$lib/components/charts/HeaterChart.svelte';
  import Input from '$lib/components/ui/input/input.svelte';
  import Switch from '$lib/components/ui/switch/switch.svelte';
  import { toast } from 'svelte-sonner';
  import parameterLabels from '$lib/parameter-labels';
  import { parameterHelpTexts } from '$lib/parameter-help-texts';
  import { get } from 'svelte/store';
  import type { Parameter } from '$lib/types/parameters';

  let interval: number | null = null;

  // Memoized filtered parameters to avoid recalculating on every render
  let brewSetpointParam = $derived(cleverCoffeeState.parameters.find((p) => p.name === "brew.setpoint"));

  let functionToggleParams = $derived(cleverCoffeeState.parameters.filter((p) =>
      ["pid.enabled", "STEAM_MODE", "BACKFLUSH_ON"].includes(p.name)
    ));

  let scaleActionParams = $derived(cleverCoffeeState.parameters.filter((p) =>
      ["TARE_ON", "CALIBRATION_ON"].includes(p.name)
    ));

  let scaleEnabled = $derived(cleverCoffeeState.parameters.some((p) => p.name === "SCALE_ENABLED" && p.value === 1));


  $effect(() => {
    console.log("Current temperature data:", cleverCoffeeState.currentTempData);
  })

  // Memoized chart data to prevent unnecessary re-renders
  let tempChartData = $derived([
      cleverCoffeeState.tempData.tempDates.map((d: Date) => d.getTime() / 1000),
      cleverCoffeeState.tempData.curTempVals,
      cleverCoffeeState.tempData.targetTempVals,
    ]);
  let heaterChartData = $derived([
      cleverCoffeeState.heaterData.heaterDates.map((d: Date) => d.getTime() / 1000),
      cleverCoffeeState.heaterData.heaterPowerVals,
    ]);

  // Helper function to get the correct step value for number inputs
  function getNumberStep(param: Parameter) {
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
  async function handleSubmitParameters() {
    if (!brewSetpointParam) return;
    const success = await saveSomeParameters([brewSetpointParam]);

    if (success) {
      toast.success("Parameters saved successfully", {
        description: "Your configuration has been updated.",
      });
    } else {
      toast.error("Failed to save parameters", {
        description: "Please check your connection and try again.",
      });
    }
  }

  // Handle function toggles by calling dedicated context methods if available
  async function handleToggleFunction(paramName: string) {
    let success = false;
    if (paramName === "pid.enabled") {
      success = await togglePid();
    } else if (paramName === "STEAM_MODE") {
      success = await toggleSteam();
    } else if (paramName === "BACKFLUSH_ON") {
      success = await toggleBackflush();
    } else {
      // Fallback: update and save parameter
      const param = cleverCoffeeState.parameters.find((p) => p.name === paramName);
      if (!param) return;
      const newValue = param.value === 1 ? 0 : 1;
      updateParameter(paramName, newValue);
      param.value
      success = await saveSomeParameters([{ name: paramName, value: newValue }]);
    }
    if (success) {
      toast.success(
        `${parameterLabels.en[paramName] || paramName} toggled successfully`,
        { description: "Setting updated via API endpoint or parameter save." }
      );
    } else {
      toast.error("Failed to toggle", {
        description: "Please check your connection and try again.",
      });
    }
  }

  // Handle scale actions by calling dedicated context methods
  async function handleScaleAction(paramName: string) {
    let success = false;
    if (paramName === "TARE_ON") {
      success = await toggleTareScale();
    } else if (paramName === "CALIBRATION_ON") {
      success = await toggleScaleCalibration();
    } else {
      updateParameter(paramName, 1);
      success = true;
    }
    if (success) {
      toast.success(
        `${parameterLabels.en[paramName] || paramName} action triggered`,
        { description: "Command executed successfully" }
      );
    } else {
      toast.error("Failed to trigger action", {
        description: "Please check your connection and try again.",
      });
    }
  }

  // Handle temperature retry with feedback
  async function handleTemperatureRetry() {
    const success = await fetchTemperatureAndChartData(true); // Show loading during manual retry

    if (success) {
      toast.success("Temperature restored", {
        description: "Successfully reconnected to temperature sensor",
      });
    } else {
      toast.error("Temperature Error", {
        description: "Unable to fetch current temperature from sensor",
      });
    }
  }

  // Handle chart data retry with feedback
  async function handleChartRetry() {
    const success = await fetchHistoryData();

    if (!success) {
      toast.error("Chart Error", {
        description: "Failed to load historical chart data",
      });
    }
  }

  onMount(() => {
    fetchHistoryData();
    interval = window.setInterval(() => {
      fetchTemperatureAndChartData(false);
    }, 5000);
    return () => {
      if (interval) {
        clearInterval(interval);
      }
    };
  });
</script>

<div class="container mx-auto p-6 space-y-6 max-w-7xl">
  <!-- Connection Error Alert -->
  {#if cleverCoffeeState.temperatureError}
    <Alert class="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive">
      <AlertCircle class="h-4 w-4" />
      <AlertDescription>
        {cleverCoffeeState.temperatureError}
        <Button
          variant="outline"
          size="sm"
          class="ml-3"
          on:click={retryConnection}
        >
          <RefreshCw class="h-4 w-4 mr-1" />
          Retry
        </Button>
      </AlertDescription>
    </Alert>
  {/if}

  <!-- Machine Status and Functions Grid -->
  <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
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
          <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            <!-- Temperature Display -->
            <div class="space-y-3">
              <div class="font-medium flex items-center gap-2">
                <Thermometer class="h-4 w-4 text-red-500" />
                Current Temperature
              </div>
              {#if cleverCoffeeState.isLoadingTemp}
                <div class="flex items-center gap-3">
                  <Skeleton class="h-10 w-20" />
                  <div class="space-y-1">
                    <Skeleton class="h-3 w-16" />
                    <Skeleton class="h-2 w-12" />
                  </div>
                </div>
              {:else if cleverCoffeeState.temperatureError}
                <div class="space-y-3">
                  <div class="flex items-center gap-2">
                    <AlertCircle class="h-5 w-5 text-destructive" />
                    <div>
                      <p class="font-semibold text-destructive">
                        {cleverCoffeeState.temperatureError.includes("offline")
                          ? "Sensor Offline"
                          : cleverCoffeeState.temperatureError.includes("retrying")
                          ? "Retrying..."
                          : "Connection Failed"}
                      </p>
                      <p class="text-xs text-muted-foreground">
                        {cleverCoffeeState.temperatureError}
                      </p>
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    on:click={handleTemperatureRetry}
                    disabled={cleverCoffeeState.temperatureError.includes("retrying")}
                  >
                    <RefreshCw
                      class={`h-4 w-4 mr-2 ${
                        cleverCoffeeState.temperatureError.includes("retrying")
                          ? "animate-spin"
                          : ""
                      }`}
                    />
                    {cleverCoffeeState.temperatureError.includes("retrying")
                      ? "Retrying..."
                      : "Retry"}
                  </Button>
                </div>
              {:else if cleverCoffeeState.currentTempData}
                <div class="space-y-1">
                  <div class="flex items-baseline gap-2">
                    <span class="text-3xl font-bold tracking-tight">
                      {cleverCoffeeState.currentTempData.currentTemp.toFixed(2)}
                    </span>
                    <span class="text-xl text-muted-foreground">
                      °C
                    </span>
                  </div>
                  <div class="flex items-center gap-1 text-xs text-green-600">
                    <div class="h-2 w-2 bg-green-500 rounded-full animate-pulse"></div>
                    Live
                  </div>
                </div>
              {:else}
                <div class="space-y-3">
                  <div class="flex items-center gap-2">
                    <AlertCircle class="h-5 w-5 text-orange-500" />
                    <div>
                      <p class="font-semibold text-orange-600">
                        No Data
                      </p>
                      <p class="text-xs text-muted-foreground">
                        Temperature sensor offline
                      </p>
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    on:click={handleTemperatureRetry}
                  >
                    <RefreshCw class="h-4 w-4 mr-2" />
                    Retry
                  </Button>
                </div>
              {/if}
            </div>

            <!-- Heater Power Display -->
            <div class="space-y-3">
              <div class="font-medium flex items-center gap-2">
                <Zap class="h-4 w-4 text-yellow-500" />
                Current Heater Power
              </div>
              {#if cleverCoffeeState.isLoadingTemp}
                <div class="flex items-center gap-3">
                  <Skeleton class="h-10 w-20" />
                  <div class="space-y-1">
                    <Skeleton class="h-3 w-16" />
                    <Skeleton className="h-2 w-12" />
                  </div>
                </div>
              {:else if cleverCoffeeState.temperatureError}
                <div class="space-y-3">
                  <div class="flex items-center gap-2">
                    <AlertCircle class="h-5 w-5 text-destructive" />
                    <div>
                      <p class="font-semibold text-destructive">
                        {cleverCoffeeState.temperatureError.includes("offline")
                          ? "Sensor Offline"
                          : cleverCoffeeState.temperatureError.includes("retrying")
                          ? "Retrying..."
                          : "Connection Failed"}
                      </p>
                      <p class="text-xs text-muted-foreground">
                        {cleverCoffeeState.temperatureError}
                      </p>
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    on:click={handleTemperatureRetry}
                    disabled={cleverCoffeeState.temperatureError.includes("retrying")}
                  >
                    <RefreshCw
                      class={`h-4 w-4 mr-2 ${
                        cleverCoffeeState.temperatureError.includes("retrying")
                          ? "animate-spin"
                          : ""
                      }`}
                    />
                    {cleverCoffeeState.temperatureError.includes("retrying")
                      ? "Retrying..."
                      : "Retry"}
                  </Button>
                </div>
              {:else if cleverCoffeeState.currentTempData}
                <div class="space-y-1">
                  <div class="flex items-baseline gap-2">
                    <span class="text-3xl font-bold tracking-tight">
                      {cleverCoffeeState.currentTempData.heaterPower.toFixed(2) || "0.00"}
                    </span>
                    <span class="text-xl text-muted-foreground">%</span>
                  </div>
                  <div class="flex items-center gap-1 text-xs text-green-600">
                    <div class="h-2 w-2 bg-green-500 rounded-full animate-pulse"></div>
                    Live
                  </div>
                </div>
              {:else}
                <div class="space-y-1">
                  <div class="flex items-baseline gap-2">
                    <span class="text-3xl font-bold tracking-tight">
                      0.0
                    </span>
                    <span class="text-xl text-muted-foreground">%</span>
                  </div>
                  <div class="flex items-center gap-1 text-xs text-muted-foreground">
                    <div class="h-2 w-2 bg-gray-400 rounded-full"></div>
                    Offline
                  </div>
                </div>
              {/if}
            </div>
          </div>

          <!-- Temperature Setpoint Control -->
          {#if cleverCoffeeState.parameters.length === 0}
            <div class="flex items-center justify-between p-4 border rounded-lg">
              <div class="space-y-1">
                <Skeleton class="h-4 w-32" />
                <Skeleton class="h-3 w-24" />
              </div>
              <Skeleton class="h-10 w-20" />
            </div>
          {:else if brewSetpointParam}
            <form onsubmit={handleSubmitParameters}>
              <div class="flex items-center justify-between p-4 border rounded-lg hover:bg-accent/50 transition-colors">
                <div class="space-y-1">
                  <div class="font-medium flex items-center gap-2">
                    <Settings class="h-4 w-4 text-green-600" />
                    {parameterLabels.en[brewSetpointParam.name] ||
                      brewSetpointParam.name}
                    {#if parameterHelpTexts[brewSetpointParam.name]}
                      <Popover>
                        <PopoverTrigger>
                          <Button
                            variant="ghost"
                            size="icon"
                            class="h-6 w-6 ml-1"
                            tabindex="0"
                          >
                            <HelpCircle class="h-4 w-4 text-muted-foreground" />
                          </Button>
                        </PopoverTrigger>
                        <PopoverContent class="w-80 text-xs">
                          {@html parameterHelpTexts[brewSetpointParam.name]}
                        </PopoverContent>
                      </Popover>
                    {/if}
                  </div>
                  <div class="text-sm text-muted-foreground">
                    Target: {brewSetpointParam.value}°C
                  </div>
                </div>
                <div class="flex items-center gap-2">
                  <Input
                    id={brewSetpointParam.name}
                    type="number"
                    step={getNumberStep(brewSetpointParam)}
                    value={String(brewSetpointParam.value)}
                    on:input={(e) =>
                      updateParameter(
                        brewSetpointParam.name,
                        parseFloat((e.target as HTMLInputElement).value)
                      )
                    }
                    min={brewSetpointParam.min}
                    max={brewSetpointParam.max}
                    class="w-20"
                  />
                  <Button type="submit" size="sm" disabled={cleverCoffeeState.isLoadingTemp}>
                    {#if cleverCoffeeState.isLoadingTemp}
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
        {#if cleverCoffeeState.parameters.length === 0}
          <div class="space-y-4">
            {#each [1, 2, 3] as i (i)}
              <div
                class="flex justify-between items-center p-4 border rounded-lg"
              >
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
                  class="flex items-center justify-between p-4 border rounded-lg hover:bg-accent/50 transition-colors"
                >
                  <div class="space-y-1">
                    <div class="font-medium">
                      {parameterLabels.en[param.name] || param.name}
                    </div>
                    <div class="text-sm text-muted-foreground">
                      {param.value === 1 ? "Enabled" : "Disabled"}
                    </div>
                  </div>
                  <Switch
                    id={param.name}
                    checked={param.value === 1}
                    on:checkedChange={() => handleToggleFunction(param.name)}
                  />
                </div>
              {/each}
            </div>

            <!-- Scale Operations -->
            {#if scaleEnabled && scaleActionParams.length > 0}
              <div class="space-y-3">
                <div class="flex items-center gap-2 pt-4 border-t">
                  <div class="flex h-8 w-8 items-center justify-center rounded-lg bg-blue-500/10">
                    <Settings class="h-4 w-4 text-blue-600" />
                  </div>
                  <h4 class="font-medium">Scale Operations</h4>
                </div>
                <div class="grid gap-3">
                  {#each scaleActionParams as param (param.name)}
                    <div
                      class="flex items-center justify-between p-4 border rounded-lg"
                    >
                      <div class="space-y-1">
                        <div class="font-medium">
                          {parameterLabels.en[param.name] || param.name}
                        </div>
                        <div class="text-sm text-muted-foreground">
                          {param.name === "TARE_ON"
                            ? "Reset scale to zero"
                            : "Calibrate scale accuracy"}
                        </div>
                      </div>
                      <Button
                        variant="outline"
                        on:click={() => handleScaleAction(param.name)}
                      >
                        {param.name === "TARE_ON"
                          ? "Tare Scale"
                          : "Start Calibration"}
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
          <PopoverTrigger >
            <Button variant="ghost" size="icon" class="h-8 w-8 ml-auto">
              <HelpCircle class="h-4 w-4" />
            </Button>
          </PopoverTrigger>
          <PopoverContent class="w-80">
            History of the boiler temperature. Drag to zoom, double-click to
            reset zoom again.
          </PopoverContent>
        </Popover>
      </CardTitle>
    </CardHeader>
    <CardContent>
      {#if cleverCoffeeState.chartError}
        <Alert>
          <AlertCircle class="h-4 w-4" />
          <AlertTitle class="flex items-center justify-between">
            <span>{cleverCoffeeState.chartError}</span>
            <Button variant="outline" size="sm" on:click={handleChartRetry}>
              <RefreshCw class="h-4 w-4 mr-2" />
              Retry
            </Button>
          </AlertTitle>
        </Alert>
      {:else if tempChartData.length === 0}
        <div class="flex items-center justify-center h-64 border border-dashed rounded-lg">
          <div class="text-center space-y-2">
            <Loader2 class="h-8 w-8 animate-spin mx-auto text-muted-foreground" />
            <p class="text-sm text-muted-foreground">
              Loading temperature data...
            </p>
          </div>
        </div>
      {:else}
        <TemperatureChart
          data={tempChartData}
          height={300}
          title="Temperature History"
        />
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
          <PopoverTrigger >
            <Button variant="ghost" size="icon" class="h-8 w-8 ml-auto">
              <HelpCircle class="h-4 w-4" />
            </Button>
          </PopoverTrigger>
          <PopoverContent class="w-80">
            History of the controlled heater power. Drag to zoom,
            double-click to reset zoom again.
          </PopoverContent>
        </Popover>
      </CardTitle>
    </CardHeader>
    <CardContent>
      {#if cleverCoffeeState.chartError}
        <Alert>
          <AlertCircle class="h-4 w-4" />
          <AlertDescription class="flex items-center justify-between">
            <span>{cleverCoffeeState.chartError}</span>
            <Button variant="outline" size="sm" on:click={handleChartRetry}>
              <RefreshCw class="h-4 w-4 mr-2" />
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      {:else if cleverCoffeeState.heaterData.heaterDates.length === 0}
        <div class="flex items-center justify-center h-64 border border-dashed rounded-lg">
          <div class="text-center space-y-2">
            <Loader2 class="h-8 w-8 animate-spin mx-auto text-muted-foreground" />
            <p class="text-sm text-muted-foreground">
              Loading heater data...
            </p>
          </div>
        </div>
      {:else}
        <HeaterChart
          data={heaterChartData}
          height={300}
          title="Heater Power History"
        />
      {/if}
    </CardContent>
  </Card>
</div>
