import React, { useMemo, useEffect } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  HelpCircle,
  Loader2,
  AlertCircle,
  RefreshCw,
  Thermometer,
  Zap,
  Settings,
  TrendingUp,
  Activity,
} from "lucide-react";
import { toast } from "sonner";
import parameterLabels from "@/lib/parameter-labels";
import { parameterHelpTexts } from "@/lib/parameter-help-texts";
import { useCleverCoffee } from "@/context/useCleverCoffee";
import TemperatureChart from "@/components/charts/TemperatureChart";
import HeaterChart from "@/components/charts/HeaterChart";

export function HomePage() {
  const {
    parameters,
    currentTempData,
    loadingParams,
    isLoadingTemp,
    temperatureError,
    chartError,
    tempData,
    heaterData,
    fetchTemperatureAndChartData,
    fetchHistoryData,
    updateParameter,
    saveParameters,
    retryConnection,
    togglePid,
    toggleSteam,
    toggleBackflush,
    toggleTareScale,
    toggleScaleCalibration,
  } = useCleverCoffee();

  useEffect(() => {
    if (fetchHistoryData) {
      fetchHistoryData();
    }
  }, [fetchHistoryData]);

  // Memoized filtered parameters to avoid recalculating on every render
  const brewSetpointParam = useMemo(
    () => parameters.find((p) => p.name === "brew.setpoint"),
    [parameters]
  );

  const functionToggleParams = useMemo(
    () =>
      parameters.filter((p) =>
        ["pid.enabled", "STEAM_MODE", "BACKFLUSH_ON"].includes(p.name)
      ),
    [parameters]
  );

  const scaleActionParams = useMemo(
    () =>
      parameters.filter((p) => ["TARE_ON", "CALIBRATION_ON"].includes(p.name)),
    [parameters]
  );

  const scaleEnabled = useMemo(
    () =>
      parameters.find((p) => p.name === "hardware.sensors.scale.enabled")
        ?.value === 1,
    [parameters]
  );

  // Memoized chart data to prevent unnecessary re-renders
  const tempChartData = useMemo(() => {
    if (tempData.tempDates.length === 0) return [];
    return [
      tempData.tempDates.map((d: Date) => d.getTime() / 1000),
      tempData.curTempVals,
      tempData.targetTempVals,
    ];
  }, [tempData.tempDates, tempData.curTempVals, tempData.targetTempVals]);

  const heaterChartData = useMemo(() => {
    if (heaterData.heaterDates.length === 0) return [];
    return [
      heaterData.heaterDates.map((d: Date) => d.getTime() / 1000),
      heaterData.heaterPowerVals,
    ];
  }, [heaterData.heaterDates, heaterData.heaterPowerVals]);

  // Helper function to get the correct step value for number inputs
  const getNumberStep = (param: { type: number }) => {
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
  };

  // Handle form submission for brew setpoint
  const handleSubmitParameters = async (e: React.FormEvent) => {
    e.preventDefault();
    const success = await saveParameters(); // Only submit brew.setpoint from home page

    if (success) {
      toast.success("Parameters saved successfully", {
        description: "Your configuration has been updated.",
      });
    } else {
      toast.error("Failed to save parameters", {
        description: "Please check your connection and try again.",
      });
    }
  };

  // Handle function toggles by calling dedicated context methods if available
  const handleToggleFunction = async (paramName: string) => {
    let success = false;
    if (paramName === "pid.enabled") {
      success = await togglePid();
    } else if (paramName === "STEAM_MODE") {
      success = await toggleSteam();
    } else if (paramName === "BACKFLUSH_ON") {
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
      toast.success(
        `${parameterLabels.en[paramName] || paramName} toggled successfully`,
        { description: "Setting updated via API endpoint or parameter save." }
      );
    } else {
      toast.error("Failed to toggle", {
        description: "Please check your connection and try again.",
      });
    }
  };

  // Handle scale actions by calling dedicated context methods
  const handleScaleAction = async (paramName: string) => {
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
  };

  // Handle temperature retry with feedback
  const handleTemperatureRetry = async () => {
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
  };

  // Handle chart data retry with feedback
  const handleChartRetry = async () => {
    const success = await fetchHistoryData();

    if (!success) {
      toast.error("Chart Error", {
        description: "Failed to load historical chart data",
      });
    }
  };

  return (
    <div className="container mx-auto p-6 space-y-6 max-w-7xl">
      {/* Connection Error Alert */}
      {temperatureError && (
        <Alert className="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            {temperatureError}
            <Button
              variant="outline"
              size="sm"
              className="ml-3"
              onClick={retryConnection}
            >
              <RefreshCw className="h-4 w-4 mr-1" />
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {/* Machine Status and Functions Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Machine Status Card */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-500/10">
                <Activity className="h-5 w-5 text-blue-600" />
              </div>
              Machine Status
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {/* Temperature Display */}
                <div className="space-y-3">
                  <div className="font-medium flex items-center gap-2">
                    <Thermometer className="h-4 w-4 text-red-500" />
                    Current Temperature
                  </div>
                  {isLoadingTemp ? (
                    <div className="flex items-center gap-3">
                      <Skeleton className="h-10 w-20" />
                      <div className="space-y-1">
                        <Skeleton className="h-3 w-16" />
                        <Skeleton className="h-2 w-12" />
                      </div>
                    </div>
                  ) : temperatureError ? (
                    <div className="space-y-3">
                      <div className="flex items-center gap-2">
                        <AlertCircle className="h-5 w-5 text-destructive" />
                        <div>
                          <p className="font-semibold text-destructive">
                            {temperatureError.includes("offline")
                              ? "Sensor Offline"
                              : temperatureError.includes("retrying")
                              ? "Retrying..."
                              : "Connection Failed"}
                          </p>
                          <p className="text-xs text-muted-foreground">
                            {temperatureError}
                          </p>
                        </div>
                      </div>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={handleTemperatureRetry}
                        disabled={temperatureError.includes("retrying")}
                      >
                        <RefreshCw
                          className={`h-4 w-4 mr-2 ${
                            temperatureError.includes("retrying")
                              ? "animate-spin"
                              : ""
                          }`}
                        />
                        {temperatureError.includes("retrying")
                          ? "Retrying..."
                          : "Retry"}
                      </Button>
                    </div>
                  ) : currentTempData ? (
                    <div className="space-y-1">
                      <div className="flex items-baseline gap-2">
                        <span className="text-3xl font-bold tracking-tight">
                          {currentTempData.currentTemp.toFixed(2)}
                        </span>
                        <span className="text-xl text-muted-foreground">
                          °C
                        </span>
                      </div>
                      <div className="flex items-center gap-1 text-xs text-green-600">
                        <div className="h-2 w-2 bg-green-500 rounded-full animate-pulse" />
                        Live
                      </div>
                    </div>
                  ) : (
                    <div className="space-y-3">
                      <div className="flex items-center gap-2">
                        <AlertCircle className="h-5 w-5 text-orange-500" />
                        <div>
                          <p className="font-semibold text-orange-600">
                            No Data
                          </p>
                          <p className="text-xs text-muted-foreground">
                            Temperature sensor offline
                          </p>
                        </div>
                      </div>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={handleTemperatureRetry}
                      >
                        <RefreshCw className="h-4 w-4 mr-2" />
                        Retry
                      </Button>
                    </div>
                  )}
                </div>

                {/* Heater Power Display */}
                <div className="space-y-3">
                  <div className="font-medium flex items-center gap-2">
                    <Zap className="h-4 w-4 text-yellow-500" />
                    Current Heater Power
                  </div>
                  {isLoadingTemp ? (
                    <div className="flex items-center gap-3">
                      <Skeleton className="h-10 w-20" />
                      <div className="space-y-1">
                        <Skeleton className="h-3 w-16" />
                        <Skeleton className="h-2 w-12" />
                      </div>
                    </div>
                  ) : temperatureError ? (
                    <div className="space-y-3">
                      <div className="flex items-center gap-2">
                        <AlertCircle className="h-5 w-5 text-destructive" />
                        <div>
                          <p className="font-semibold text-destructive">
                            {temperatureError.includes("offline")
                              ? "Sensor Offline"
                              : temperatureError.includes("retrying")
                              ? "Retrying..."
                              : "Connection Failed"}
                          </p>
                          <p className="text-xs text-muted-foreground">
                            {temperatureError}
                          </p>
                        </div>
                      </div>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={handleTemperatureRetry}
                        disabled={temperatureError.includes("retrying")}
                      >
                        <RefreshCw
                          className={`h-4 w-4 mr-2 ${
                            temperatureError.includes("retrying")
                              ? "animate-spin"
                              : ""
                          }`}
                        />
                        {temperatureError.includes("retrying")
                          ? "Retrying..."
                          : "Retry"}
                      </Button>
                    </div>
                  ) : currentTempData ? (
                    <div className="space-y-1">
                      <div className="flex items-baseline gap-2">
                        <span className="text-3xl font-bold tracking-tight">
                          {currentTempData.heaterPower.toFixed(2) || "0.00"}
                        </span>
                        <span className="text-xl text-muted-foreground">%</span>
                      </div>
                      <div className="flex items-center gap-1 text-xs text-green-600">
                        <div className="h-2 w-2 bg-green-500 rounded-full animate-pulse" />
                        Live
                      </div>
                    </div>
                  ) : (
                    <div className="space-y-1">
                      <div className="flex items-baseline gap-2">
                        <span className="text-3xl font-bold tracking-tight">
                          0.0
                        </span>
                        <span className="text-xl text-muted-foreground">%</span>
                      </div>
                      <div className="flex items-center gap-1 text-xs text-muted-foreground">
                        <div className="h-2 w-2 bg-gray-400 rounded-full" />
                        Offline
                      </div>
                    </div>
                  )}
                </div>
              </div>

              {/* Temperature Setpoint Control */}
              {loadingParams ? (
                <div className="flex items-center justify-between p-4 border rounded-lg">
                  <div className="space-y-1">
                    <Skeleton className="h-4 w-32" />
                    <Skeleton className="h-3 w-24" />
                  </div>
                  <Skeleton className="h-10 w-20" />
                </div>
              ) : brewSetpointParam ? (
                <form onSubmit={handleSubmitParameters}>
                  <div className="flex items-center justify-between p-4 border rounded-lg hover:bg-accent/50 transition-colors">
                    <div className="space-y-1">
                      <div className="font-medium flex items-center gap-2">
                        <Settings className="h-4 w-4 text-green-600" />
                        {parameterLabels.en[brewSetpointParam.name] ||
                          brewSetpointParam.name}
                        {parameterHelpTexts[brewSetpointParam.name] && (
                          <Popover>
                            <PopoverTrigger asChild>
                              <Button
                                variant="ghost"
                                size="icon"
                                className="h-6 w-6 ml-1"
                                tabIndex={0}
                                onMouseEnter={() =>
                                  parameterHelpTexts[brewSetpointParam.name]
                                }
                              >
                                <HelpCircle className="h-4 w-4 text-muted-foreground" />
                              </Button>
                            </PopoverTrigger>
                            <PopoverContent className="w-80 text-xs">
                              <span
                                dangerouslySetInnerHTML={{
                                  __html:
                                    parameterHelpTexts[
                                      brewSetpointParam.name
                                    ] || "Loading help text...",
                                }}
                              />
                            </PopoverContent>
                          </Popover>
                        )}
                      </div>
                      <div className="text-sm text-muted-foreground">
                        Target: {brewSetpointParam.value}°C
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <Input
                        id={brewSetpointParam.name}
                        type="number"
                        step={getNumberStep(brewSetpointParam)}
                        value={brewSetpointParam.value as string}
                        onChange={(e) =>
                          updateParameter(
                            brewSetpointParam.name,
                            e.target.value
                          )
                        }
                        min={brewSetpointParam.min}
                        max={brewSetpointParam.max}
                        className="w-20"
                      />
                      <Button type="submit" size="sm" disabled={isLoadingTemp}>
                        {isLoadingTemp && (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        )}
                        {!isLoadingTemp && "Save"}
                      </Button>
                    </div>
                  </div>
                </form>
              ) : null}
            </div>
          </CardContent>
        </Card>

        {/* Machine Functions Card */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-purple-500/10">
                <Zap className="h-5 w-5 text-purple-600" />
              </div>
              Machine Functions
            </CardTitle>
          </CardHeader>
          <CardContent>
            {loadingParams ? (
              <div className="space-y-4">
                {[1, 2, 3].map((i) => (
                  <div
                    key={i}
                    className="flex justify-between items-center p-4 border rounded-lg"
                  >
                    <Skeleton className="h-5 w-32" />
                    <Skeleton className="h-6 w-12" />
                  </div>
                ))}
              </div>
            ) : (
              <div className="space-y-4">
                {/* Function Toggles */}
                <div className="grid gap-3">
                  {functionToggleParams.map((param) => (
                    <div
                      key={param.name}
                      className="flex items-center justify-between p-4 border rounded-lg hover:bg-accent/50 transition-colors"
                    >
                      <div className="space-y-1">
                        <div className="font-medium">
                          {parameterLabels.en[param.name] || param.name}
                        </div>
                        <div className="text-sm text-muted-foreground">
                          {param.value === 1 ? "Enabled" : "Disabled"}
                        </div>
                      </div>
                      <Switch
                        id={param.name}
                        checked={param.value === 1}
                        onCheckedChange={() => handleToggleFunction(param.name)}
                      />
                    </div>
                  ))}
                </div>

                {/* Scale Operations */}
                {scaleEnabled && scaleActionParams.length > 0 && (
                  <div className="space-y-3">
                    <div className="flex items-center gap-2 pt-4 border-t">
                      <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-blue-500/10">
                        <Settings className="h-4 w-4 text-blue-600" />
                      </div>
                      <h4 className="font-medium">Scale Operations</h4>
                    </div>
                    <div className="grid gap-3">
                      {scaleActionParams.map((param) => (
                        <div
                          key={param.name}
                          className="flex items-center justify-between p-4 border rounded-lg"
                        >
                          <div className="space-y-1">
                            <div className="font-medium">
                              {parameterLabels.en[param.name] || param.name}
                            </div>
                            <div className="text-sm text-muted-foreground">
                              {param.name === "TARE_ON"
                                ? "Reset scale to zero"
                                : "Calibrate scale accuracy"}
                            </div>
                          </div>
                          <Button
                            variant="outline"
                            onClick={() => handleScaleAction(param.name)}
                          >
                            {param.name === "TARE_ON"
                              ? "Tare Scale"
                              : "Start Calibration"}
                          </Button>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Temperature Chart */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-red-500/10">
              <TrendingUp className="h-5 w-5 text-red-600" />
            </div>
            Temperature History
            <Popover>
              <PopoverTrigger asChild>
                <Button variant="ghost" size="icon" className="h-8 w-8 ml-auto">
                  <HelpCircle className="h-4 w-4" />
                </Button>
              </PopoverTrigger>
              <PopoverContent className="w-80">
                History of the boiler temperature. Drag to zoom, double-click to
                reset zoom again.
              </PopoverContent>
            </Popover>
          </CardTitle>
        </CardHeader>
        <CardContent>
          {chartError ? (
            <Alert>
              <AlertCircle className="h-4 w-4" />
              <AlertTitle className="flex items-center justify-between">
                <span>{chartError}</span>
                <Button variant="outline" size="sm" onClick={handleChartRetry}>
                  <RefreshCw className="h-4 w-4 mr-2" />
                  Retry
                </Button>
              </AlertTitle>
            </Alert>
          ) : tempChartData.length === 0 ? (
            <div className="flex items-center justify-center h-64 border border-dashed rounded-lg">
              <div className="text-center space-y-2">
                <Loader2 className="h-8 w-8 animate-spin mx-auto text-muted-foreground" />
                <p className="text-sm text-muted-foreground">
                  Loading temperature data...
                </p>
              </div>
            </div>
          ) : (
            <TemperatureChart
              data={tempChartData}
              height={300}
              title="Temperature History"
            />
          )}
        </CardContent>
      </Card>

      {/* Heater Chart */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-yellow-500/10">
              <Zap className="h-5 w-5 text-yellow-600" />
            </div>
            Heater Power History
            <Popover>
              <PopoverTrigger asChild>
                <Button variant="ghost" size="icon" className="h-8 w-8 ml-auto">
                  <HelpCircle className="h-4 w-4" />
                </Button>
              </PopoverTrigger>
              <PopoverContent className="w-80">
                History of the controlled heater power. Drag to zoom,
                double-click to reset zoom again.
              </PopoverContent>
            </Popover>
          </CardTitle>
        </CardHeader>
        <CardContent>
          {chartError ? (
            <Alert>
              <AlertCircle className="h-4 w-4" />
              <AlertDescription className="flex items-center justify-between">
                <span>{chartError}</span>
                <Button variant="outline" size="sm" onClick={handleChartRetry}>
                  <RefreshCw className="h-4 w-4 mr-2" />
                  Retry
                </Button>
              </AlertDescription>
            </Alert>
          ) : heaterChartData.length === 0 ? (
            <div className="flex items-center justify-center h-64 border border-dashed rounded-lg">
              <div className="text-center space-y-2">
                <Loader2 className="h-8 w-8 animate-spin mx-auto text-muted-foreground" />
                <p className="text-sm text-muted-foreground">
                  Loading heater data...
                </p>
              </div>
            </div>
          ) : (
            <HeaterChart
              data={heaterChartData}
              height={300}
              title="Heater Power History"
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
