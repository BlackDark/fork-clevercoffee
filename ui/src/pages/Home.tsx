import { useMemo, useEffect, useRef } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { TemperatureChart } from "@/components/charts/TemperatureChart";
import { HeaterChart } from "@/components/charts/HeaterChart";
import { useCleverCoffee } from "@/hooks/use-clever-coffee";
import {
  HelpCircle,
  Loader2,
  CheckCircle,
  AlertCircle,
  RefreshCw,
  Thermometer,
  Zap,
  Settings,
  TrendingUp,
  Activity,
} from "lucide-react";
import { toast } from "sonner";

export function Home() {
  // Use the centralized hook for all data and actions
  const {
    // State
    parameters,
    currentTemperature,
    parametersHelpTexts,
    isLoadingParams,
    isLoadingTemp,
    isPostingForm,
    showPostSucceeded,
    connectionError,
    temperatureError,
    chartError,
    tempData,
    heaterData,

    // Actions
    fetchTemperatureAndChartData,
    fetchHistoryData,
    fetchHelpText,
    updateParameterValue,
    postParameters,
    togglePid,
    toggleSteamMode,
    toggleBackflush,
    tareScale,
    calibrateScale,
    clearTemperatureError,
    clearChartError,
    retryConnection,
  } = useCleverCoffee();

  // Polling interval ref for temperature and chart data
  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // Setup polling for temperature and chart data (only needed on Home page)
  useEffect(() => {
    // Start polling every 5 seconds (reduced from 2 seconds to minimize flicker)
    pollingIntervalRef.current = setInterval(async () => {
      await fetchTemperatureAndChartData(); // Don't show loading during polling
    }, 5000);

    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
        pollingIntervalRef.current = null;
      }
    };
  }, [fetchTemperatureAndChartData]);

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

  // Memoized chart data to prevent unnecessary re-renders
  const tempChartData = useMemo(() => {
    if (tempData.tempDates.length === 0) return [];
    return [
      tempData.tempDates.map((d) => d.getTime() / 1000),
      tempData.curTempVals,
      tempData.targetTempVals,
    ];
  }, [tempData.tempDates, tempData.curTempVals, tempData.targetTempVals]);

  const heaterChartData = useMemo(() => {
    if (heaterData.heaterDates.length === 0) return [];
    return [
      heaterData.heaterDates.map((d) => d.getTime() / 1000),
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

  // Handle form submission
  const handleSubmitParameters = async (e: React.FormEvent) => {
    e.preventDefault();
    const success = await postParameters(["brew.setpoint"]); // Only submit brew.setpoint from home page

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

  // Handle function toggles with proper mapping and toast feedback
  const handleToggleFunction = async (paramName: string) => {
    let success = false;
    let displayName = paramName;

    // Find the parameter for display name
    const param = parameters.find((p) => p.name === paramName);
    if (param) {
      displayName = param.displayName || paramName;
    }

    switch (paramName) {
      case "pid.enabled":
        success = await togglePid();
        break;
      case "STEAM_MODE":
        success = await toggleSteamMode();
        break;
      case "BACKFLUSH_ON":
        success = await toggleBackflush();
        break;
    }

    if (success) {
      const newValue = param?.value === 1 ? 0 : 1;
      toast.success(`${displayName} ${newValue ? "enabled" : "disabled"}`, {
        description: "Setting updated successfully",
      });
    } else {
      toast.error("Failed to update setting", {
        description: "Please check your connection and try again.",
      });
    }
  };

  // Handle temperature retry with feedback
  const handleTemperatureRetry = async () => {
    clearTemperatureError();
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
    clearChartError();
    const success = await fetchHistoryData();

    if (!success) {
      toast.error("Chart Error", {
        description: "Failed to load historical chart data",
      });
    }
  };

  // Handle scale actions with feedback
  const handleTareScale = async () => {
    const success = await tareScale();

    if (success) {
      toast.success("Scale tared", {
        description: "Command executed successfully",
      });
    } else {
      toast.error("Failed to execute action", {
        description: "Please check your connection and try again.",
      });
    }
  };

  const handleCalibrateScale = async () => {
    const success = await calibrateScale();

    if (success) {
      toast.success("Scale calibration started", {
        description: "Command executed successfully",
      });
    } else if (success === false) {
      // Only show error if the action was attempted but failed
      // (calibrateScale returns false if user cancels, which shouldn't show error)
      toast.error("Failed to execute action", {
        description: "Please check your connection and try again.",
      });
    }
  };

  return (
    <div className="container mx-auto p-6 space-y-6 max-w-7xl">
      {/* Connection Error Alert */}
      {connectionError && (
        <Alert className="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            {connectionError}
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
          <form onSubmit={handleSubmitParameters}>
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
              {/* Temperature Display */}
              <div className="space-y-3">
                <Label className="text-sm font-medium flex items-center gap-2">
                  <Thermometer className="h-4 w-4 text-red-500" />
                  Current Temperature
                </Label>
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
                ) : currentTemperature ? (
                  <div className="space-y-1">
                    <div className="flex items-baseline gap-2">
                      <span className="text-3xl font-bold tracking-tight">
                        {currentTemperature}
                      </span>
                      <span className="text-xl text-muted-foreground">°C</span>
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
                        <p className="font-semibold text-orange-600">No Data</p>
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

              {/* Setpoint Parameter */}
              {isLoadingParams ? (
                <div className="space-y-3">
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-10 w-32" />
                </div>
              ) : brewSetpointParam ? (
                <div className="space-y-3">
                  <Label
                    htmlFor={brewSetpointParam.name}
                    className="text-sm font-medium flex items-center gap-2"
                  >
                    <Settings className="h-4 w-4 text-green-600" />
                    {brewSetpointParam.displayName}
                    {brewSetpointParam.hasHelpText && (
                      <Popover>
                        <PopoverTrigger asChild>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-6 w-6"
                            onMouseEnter={() =>
                              fetchHelpText(brewSetpointParam.name)
                            }
                          >
                            <HelpCircle className="h-4 w-4" />
                          </Button>
                        </PopoverTrigger>
                        <PopoverContent className="w-80">
                          {parametersHelpTexts[brewSetpointParam.name] ||
                            "Loading help text..."}
                        </PopoverContent>
                      </Popover>
                    )}
                  </Label>
                  <Input
                    id={brewSetpointParam.name}
                    type="number"
                    step={getNumberStep(brewSetpointParam)}
                    value={brewSetpointParam.value as string}
                    onChange={(e) =>
                      updateParameterValue(
                        brewSetpointParam.name,
                        e.target.value
                      )
                    }
                    min={brewSetpointParam.min}
                    max={brewSetpointParam.max}
                    className="w-32"
                  />
                </div>
              ) : null}

              {/* Save Button */}
              <div className="flex justify-end">
                <Button
                  type="submit"
                  disabled={isPostingForm}
                  className="min-w-32"
                >
                  {showPostSucceeded && (
                    <CheckCircle className="mr-2 h-4 w-4" />
                  )}
                  {isPostingForm && (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  )}
                  {showPostSucceeded ? "Saved!" : "Save Changes"}
                </Button>
              </div>
            </div>
          </form>
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
          {isLoadingParams ? (
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
                        {param.displayName || param.name}
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
              {scaleActionParams.length > 0 && (
                <div className="space-y-3">
                  <div className="flex items-center gap-2 pt-4 border-t">
                    <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-blue-500/10">
                      <Settings className="h-4 w-4 text-blue-600" />
                    </div>
                    <h4 className="font-medium">Scale Operations</h4>
                  </div>
                  <div className="grid gap-3">
                    {scaleActionParams.map((param) => {
                      if (param.name === "TARE_ON") {
                        return (
                          <div
                            key={param.name}
                            className="flex items-center justify-between p-4 border rounded-lg"
                          >
                            <div className="space-y-1">
                              <div className="font-medium">Tare Scale</div>
                              <div className="text-sm text-muted-foreground">
                                Reset scale to zero
                              </div>
                            </div>
                            <Button variant="outline" onClick={handleTareScale}>
                              Tare Scale
                            </Button>
                          </div>
                        );
                      }
                      if (param.name === "CALIBRATION_ON") {
                        return (
                          <div
                            key={param.name}
                            className="flex items-center justify-between p-4 border rounded-lg"
                          >
                            <div className="space-y-1">
                              <div className="font-medium">
                                Scale Calibration
                              </div>
                              <div className="text-sm text-muted-foreground">
                                Calibrate scale accuracy
                              </div>
                            </div>
                            <Button
                              variant="outline"
                              onClick={handleCalibrateScale}
                            >
                              Start Calibration
                            </Button>
                          </div>
                        );
                      }
                      return null;
                    })}
                  </div>
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

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
            <div className="w-full">
              <TemperatureChart
                data={tempChartData}
                height={300}
                title="Temperature History"
              />
            </div>
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
            <div className="w-full">
              <HeaterChart
                data={heaterChartData}
                height={300}
                title="Heater Power History"
              />
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
