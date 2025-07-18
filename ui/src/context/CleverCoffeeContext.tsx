import React, { useState, useCallback, useEffect, useMemo } from "react";
import { apiFetch } from "@/lib/api-config";
import type { Parameter } from "../lib/parameter-types";
import { groupParametersBySection } from "../lib/parameter-utils";
import { ensureCompleteParameters } from "@/lib/parameter-metadata";
import type { HistoryData, TemperatureData } from "@/types/api";
import { CleverCoffeeContext } from "@/context/useCleverCoffee";

interface CleverCoffeeContextValue {
  // Parameters
  parameters: Parameter[];
  parametersBySection: Record<string, Parameter[]>;
  loadingParams: boolean;
  errorParams: string | null;
  updateParameter: (name: string, value: string | number | boolean) => void;
  saveParameters: () => Promise<boolean>;
  refreshParameters: () => Promise<void>;
  getParameter: (name: string) => Parameter | undefined;

  // Chart
  tempData: {
    tempDates: Date[];
    curTempVals: number[];
    targetTempVals: number[];
  };
  heaterData: {
    heaterDates: Date[];
    heaterPowerVals: number[];
  };
  chartError: string | null;
  isHistoryLoaded: boolean;
  fetchHistoryData: () => Promise<boolean>;

  // Health
  isOnline: boolean;
  lastHealthCheck: Date | null;
  connectionError: string | null;
  checkHealth: () => Promise<boolean>;

  // Temperature
  currentTemperature: string | null;
  isLoadingTemp: boolean;
  temperatureError: string | null;
  fetchTemperatureAndChartData: (showLoading?: boolean) => Promise<boolean>;

  // Retry
  retryConnection: () => void;

  // Dedicated toggles
  togglePid: () => Promise<boolean>;
  toggleSteam: () => Promise<boolean>;
  toggleBackflush: () => Promise<boolean>;
  toggleTareScale: () => Promise<boolean>;
  toggleScaleCalibration: () => Promise<boolean>;
}

export const CleverCoffeeProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  // Parameters
  const [parameters, setParameters] = useState<Parameter[]>([]);
  const [loadingParams, setLoadingParams] = useState(true);
  const [errorParams, setErrorParams] = useState<string | null>(null);

  // Chart (local state)
  const [tempData, setTempData] = useState<{
    tempDates: Date[];
    curTempVals: number[];
    targetTempVals: number[];
  }>({ tempDates: [], curTempVals: [], targetTempVals: [] });
  const [heaterData, setHeaterData] = useState<{
    heaterDates: Date[];
    heaterPowerVals: number[];
  }>({ heaterDates: [], heaterPowerVals: [] });
  const [chartError, setChartError] = useState<string | null>(null);

  // Health
  const [isOnline, setIsOnline] = useState(true);
  const [lastHealthCheck, setLastHealthCheck] = useState<Date | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);

  // Temperature
  const [currentTemperature, setCurrentTemperature] = useState<string | null>(
    null
  );
  const [isLoadingTemp, setIsLoadingTemp] = useState(true);
  const [temperatureError, setTemperatureError] = useState<string | null>(null);

  // Track if history has been loaded
  const [isHistoryLoaded, setHistoryLoaded] = useState(false);

  const addTempData2 = useCallback(
    (data: { currentTemp?: number; targetTemp?: number }) => {
      setTempData((prev) => {
        // If only single value, append to array
        if (data.currentTemp != null && data.targetTemp != null) {
          const now = new Date();
          return {
            tempDates: [...prev.tempDates, now],
            curTempVals: [...prev.curTempVals, data.currentTemp],
            targetTempVals: [...prev.targetTempVals, data.targetTemp],
          };
        }
        return prev;
      });
    },
    []
  );

  const addHeaterData2 = useCallback((data: { heaterPower?: number }) => {
    setHeaterData((prev) => {
      // If only single value, append to array
      if (data.heaterPower !== undefined) {
        const now = new Date();
        return {
          heaterDates: [...prev.heaterDates, now],
          heaterPowerVals: [...prev.heaterPowerVals, data.heaterPower],
        };
      }
      return prev;
    });
  }, []);

  // Fetch parameters from API
  const fetchParameters = useCallback(async () => {
    try {
      setLoadingParams(true);
      setErrorParams(null);
      const response = await apiFetch("/parameters");
      if (!response.ok)
        throw new Error(`HTTP error! status: ${response.status}`);
      const data = await response.json();
      // Merge with metadata/defaults
      setParameters(
        ensureCompleteParameters(data).sort((a, b) =>
          a.name.localeCompare(b.name)
        )
      );
    } catch (err) {
      setErrorParams(
        err instanceof Error ? err.message : "Failed to fetch parameters"
      );
    } finally {
      setLoadingParams(false);
    }
  }, []);

  // Chart history
  const fetchHistoryData = useCallback(async (): Promise<boolean> => {
    try {
      setChartError(null);
      const response = await apiFetch("/history");
      if (!response.ok)
        throw new Error(`HTTP error! status: ${response.status}`);

      const historyData: HistoryData = await response.json();

      console.log("[fetchHistoryData] historyData:", historyData);

      const dates: Date[] = [];

      for (let i = historyData.heaterPowers.length; i > 0; i--) {
        const date = new Date();
        date.setSeconds(date.getSeconds() - 3 * i);
        dates.push(date);
      }

      setTempData({
        tempDates: dates,
        curTempVals: historyData.currentTemps,
        targetTempVals: historyData.targetTemps,
      });

      setHeaterData({
        heaterDates: dates,
        heaterPowerVals: historyData.heaterPowers,
      });

      setHistoryLoaded(true);
      return true;
    } catch (err) {
      setChartError("Failed to load chart history data");
      console.error("[fetchHistoryData] error:", err);
      setHistoryLoaded(false);
      return false;
    }
  }, []);

  // Initial load: fetch parameters only
  useEffect(() => {
    fetchParameters();
  }, [fetchParameters]);

  // Update a parameter value locally
  const updateParameter = useCallback(
    (name: string, value: string | number | boolean) => {
      setParameters((prev) =>
        prev.map((param) => (param.name === name ? { ...param, value } : param))
      );
    },
    []
  );

  // Save parameters to backend
  const saveParameters = useCallback(async (): Promise<boolean> => {
    try {
      setErrorParams(null);
      const formData = new URLSearchParams();
      parameters.forEach((param) => {
        formData.append(param.name, String(param.value));
      });
      const response = await apiFetch("/parameters", {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: formData.toString(),
      });
      if (!response.ok)
        throw new Error(`HTTP error! status: ${response.status}`);
      await fetchParameters();
      return true;
    } catch (err) {
      setErrorParams(
        err instanceof Error ? err.message : "Failed to save parameters"
      );
      return false;
    }
  }, [parameters, fetchParameters]);

  // Refresh parameters from backend
  const refreshParameters = useCallback(async () => {
    await fetchParameters();
  }, [fetchParameters]);

  // Get a specific parameter by name
  const getParameter = useCallback(
    (name: string): Parameter | undefined => {
      return parameters.find((param) => param.name === name);
    },
    [parameters]
  );

  // Group parameters by section
  const parametersBySection = useMemo(
    () => groupParametersBySection(parameters),
    [parameters]
  );

  // Health check
  const checkHealth = useCallback(async (): Promise<boolean> => {
    try {
      const response = await apiFetch("/alive", {
        method: "HEAD",
        signal: AbortSignal.timeout(3000),
      });
      const isHealthy = response.ok;
      setIsOnline(isHealthy);
      setLastHealthCheck(new Date());
      setConnectionError(isHealthy ? null : "Service unavailable");
      return isHealthy;
    } catch {
      setIsOnline(false);
      setLastHealthCheck(new Date());
      setConnectionError("Connection failed");
      return false;
    }
  }, []);

  // Health polling
  useEffect(() => {
    const healthCheckInterval = setInterval(async () => {
      await checkHealth();
    }, 1000);
    return () => clearInterval(healthCheckInterval);
  }, [checkHealth]);

  // Unified temperature and chart data fetching
  const fetchTemperatureAndChartData = useCallback(
    async (showLoading = false): Promise<boolean> => {
      try {
        setTemperatureError(null);
        if (showLoading) {
          setIsLoadingTemp(true);
        }
        const response = await apiFetch("/temperatures", {
          signal: AbortSignal.timeout(5000),
        });
        if (!response.ok)
          throw new Error(`HTTP error! status: ${response.status}`);
        const { currentTemp, heaterPower, targetTemp }: TemperatureData =
          await response.json();
        console.log(
          "[fetchTemperatureAndChartData] liveData:",
          currentTemp,
          heaterPower,
          targetTemp
        );

        if (currentTemp !== undefined) {
          setCurrentTemperature(currentTemp.toString());
          setTemperatureError(null);
          setIsLoadingTemp(false);
        }

        // Only append if history has loaded
        if (isHistoryLoaded) {
          if (targetTemp != null && currentTemp != null) {
            addTempData2({
              currentTemp: currentTemp,
              targetTemp: targetTemp,
            });
          }

          if (heaterPower !== undefined) {
            addHeaterData2({ heaterPower: heaterPower });
          }
        }
        return true;
      } catch (err) {
        setCurrentTemperature(null);
        setTemperatureError("Temperature sensor offline");
        if (showLoading) {
          setIsLoadingTemp(false);
        }
        console.error("[fetchTemperatureAndChartData] error:", err);
        return false;
      }
    },
    [addTempData2, addHeaterData2, isHistoryLoaded]
  );

  // Retry connection
  const retryConnection = useCallback(() => {
    setConnectionError(null);
    fetchTemperatureAndChartData(true);
    fetchParameters();
  }, [fetchTemperatureAndChartData, fetchParameters]);

  // Dedicated toggle functions
  const togglePid = useCallback(async () => {
    try {
      const response = await apiFetch("/pid", { method: "POST" });
      if (response.ok) {
        await refreshParameters();
        return true;
      }
      return false;
    } catch {
      return false;
    }
  }, [refreshParameters]);

  const toggleSteam = useCallback(async () => {
    try {
      const response = await apiFetch("/steam", { method: "POST" });
      if (response.ok) {
        await refreshParameters();
        return true;
      }
      return false;
    } catch {
      return false;
    }
  }, [refreshParameters]);

  const toggleBackflush = useCallback(async () => {
    try {
      const response = await apiFetch("/backflush", { method: "POST" });
      if (response.ok) {
        await refreshParameters();
        return true;
      }
      return false;
    } catch {
      return false;
    }
  }, [refreshParameters]);

  const toggleTareScale = useCallback(async () => {
    try {
      const response = await apiFetch("/scale/tare", { method: "POST" });
      if (response.ok) {
        await refreshParameters();
        return true;
      }
      return false;
    } catch {
      return false;
    }
  }, [refreshParameters]);

  const toggleScaleCalibration = useCallback(async () => {
    try {
      const response = await apiFetch("/scale/calibration", {
        method: "POST",
      });
      if (response.ok) {
        await refreshParameters();
        return true;
      }
      return false;
    } catch {
      return false;
    }
  }, [refreshParameters]);

  const value: CleverCoffeeContextValue = {
    parameters,
    parametersBySection,
    loadingParams,
    errorParams,
    updateParameter,
    saveParameters,
    refreshParameters,
    getParameter,
    tempData,
    heaterData,
    chartError,
    isHistoryLoaded,
    fetchHistoryData,
    isOnline,
    lastHealthCheck,
    connectionError,
    checkHealth,
    currentTemperature,
    isLoadingTemp,
    temperatureError,
    fetchTemperatureAndChartData,
    retryConnection,
    togglePid,
    toggleSteam,
    toggleBackflush,
    toggleTareScale,
    toggleScaleCalibration,
  };

  return (
    <CleverCoffeeContext.Provider value={value}>
      {children}
    </CleverCoffeeContext.Provider>
  );
};

export type { CleverCoffeeContextValue };
