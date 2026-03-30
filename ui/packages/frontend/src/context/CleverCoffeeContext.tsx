import React, { useState, useCallback, useEffect } from "react";
import { apiFetch } from "@/lib/api-config";
import { API_ROUTES } from "@/lib/routes";
import type { HistoryData, TemperatureData } from "@/types/api";
import { CleverCoffeeContext } from "@/context/useCleverCoffee";
import { useParametersApi } from "@/hooks/useParametersApi";
import { useTemperatureStream } from "@/hooks/useTemperatureStream";
import { useMachineToggles } from "@/hooks/useMachineToggles";

interface CleverCoffeeContextValue {
  parameters: ReturnType<typeof useParametersApi>["parameters"];
  parametersBySection: ReturnType<typeof useParametersApi>["parametersBySection"];
  loadingParams: ReturnType<typeof useParametersApi>["loading"];
  errorParams: ReturnType<typeof useParametersApi>["error"];
  updateParameter: ReturnType<typeof useParametersApi>["updateParameter"];
  saveParameters: ReturnType<typeof useParametersApi>["saveParameters"];
  fetchParameters: ReturnType<typeof useParametersApi>["refetch"];
  getParameter: ReturnType<typeof useParametersApi>["getParameter"];

  tempData: { tempDates: Date[]; curTempVals: number[]; targetTempVals: number[] };
  heaterData: { heaterDates: Date[]; heaterPowerVals: number[] };
  chartError: string | null;
  isHistoryLoaded: boolean;
  fetchHistoryData: () => Promise<boolean>;

  isOnline: boolean;
  lastHealthCheck: Date | null;
  connectionError: string | null;
  checkHealth: () => Promise<boolean>;

  currentTempData: TemperatureData | null;
  isLoadingTemp: boolean;
  temperatureError: string | null;
  fetchTemperatureAndChartData: (showLoading?: boolean) => Promise<boolean>;

  retryConnection: () => void;

  togglePid: ReturnType<typeof useMachineToggles>["togglePid"];
  toggleSteam: ReturnType<typeof useMachineToggles>["toggleSteam"];
  toggleBackflush: ReturnType<typeof useMachineToggles>["toggleBackflush"];
  toggleTareScale: ReturnType<typeof useMachineToggles>["toggleTareScale"];
  toggleScaleCalibration: ReturnType<typeof useMachineToggles>["toggleScaleCalibration"];
  wakeFromStandby: ReturnType<typeof useMachineToggles>["wakeFromStandby"];
}

export const CleverCoffeeProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
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
  const [isHistoryLoaded, setHistoryLoaded] = useState(false);
  const [isOnline, setIsOnline] = useState(true);
  const [lastHealthCheck, setLastHealthCheck] = useState<Date | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);

  const {
    parameters,
    parametersBySection,
    loading: loadingParams,
    error: errorParams,
    updateParameter,
    saveParameters,
    refetch: fetchParameters,
    getParameter,
  } = useParametersApi();

  const {
    temperatureData: currentTempData,
    isLoading: isLoadingTemp,
    error: temperatureError,
    refetch: refetchTemperature,
  } = useTemperatureStream(isHistoryLoaded);

  const {
    togglePid: rawTogglePid,
    toggleSteam: rawToggleSteam,
    toggleBackflush: rawToggleBackflush,
    toggleTareScale: rawToggleTareScale,
    toggleScaleCalibration: rawToggleScaleCalibration,
    wakeFromStandby: rawWakeFromStandby,
  } = useMachineToggles();

  const togglePid = useCallback(async () => {
    const success = await rawTogglePid();
    if (success) { fetchParameters(false); }
    return success;
  }, [rawTogglePid, fetchParameters]);
  const toggleSteam = useCallback(async () => {
    const success = await rawToggleSteam();
    if (success) { fetchParameters(false); }
    return success;
  }, [rawToggleSteam, fetchParameters]);
  const toggleBackflush = useCallback(async () => {
    const success = await rawToggleBackflush();
    if (success) { fetchParameters(false); }
    return success;
  }, [rawToggleBackflush, fetchParameters]);
  const toggleTareScale = useCallback(async () => {
    const success = await rawToggleTareScale();
    if (success) { fetchParameters(false); }
    return success;
  }, [rawToggleTareScale, fetchParameters]);
  const toggleScaleCalibration = useCallback(async () => {
    const success = await rawToggleScaleCalibration();
    if (success) { fetchParameters(false); }
    return success;
  }, [rawToggleScaleCalibration, fetchParameters]);

  const wakeFromStandby = useCallback(async () => {
    return await rawWakeFromStandby();
  }, [rawWakeFromStandby]);

  const addTempData = useCallback(
    (data: { currentTemp?: number; targetTemp?: number }) => {
      if (data.currentTemp != null && data.targetTemp != null) {
        const now = new Date();
        setTempData((prev) => ({
          tempDates: [...prev.tempDates, now],
          curTempVals: [...prev.curTempVals, data.currentTemp!],
          targetTempVals: [...prev.targetTempVals, data.targetTemp!],
        }));
      }
    },
    []
  );

  const addHeaterData = useCallback((data: { heaterPower?: number }) => {
    if (data.heaterPower !== undefined) {
      const now = new Date();
      setHeaterData((prev) => ({
        heaterDates: [...prev.heaterDates, now],
        heaterPowerVals: [...prev.heaterPowerVals, data.heaterPower!],
      }));
    }
  }, []);

  useEffect(() => {
    if (isHistoryLoaded && currentTempData) {
      addTempData({ currentTemp: currentTempData.currentTemp, targetTemp: currentTempData.targetTemp });
      addHeaterData({ heaterPower: currentTempData.heaterPower });
    }
  }, [isHistoryLoaded, currentTempData, addTempData, addHeaterData]);

  const fetchHistoryData = useCallback(async (): Promise<boolean> => {
    try {
      setChartError(null);
      const response = await apiFetch(API_ROUTES.HISTORY);
      if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
      const historyData: HistoryData = await response.json();

      const dates: Date[] = [];
      for (let i = historyData.heaterPowers.length; i > 0; i--) {
        const date = new Date();
        date.setSeconds(date.getSeconds() - 3 * i);
        dates.push(date);
      }

      setTempData({ tempDates: dates, curTempVals: historyData.currentTemps, targetTempVals: historyData.targetTemps });
      setHeaterData({ heaterDates: dates, heaterPowerVals: historyData.heaterPowers });
      setHistoryLoaded(true);
      return true;
    } catch {
      setChartError("Failed to load chart history data");
      setHistoryLoaded(false);
      return false;
    }
  }, []);

  const checkHealth = useCallback(async (): Promise<boolean> => {
    try {
      const response = await apiFetch(API_ROUTES.HEALTH, { signal: AbortSignal.timeout(3000) });
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

  const fetchTemperatureAndChartData = useCallback(
    async (showLoading = false): Promise<boolean> => {
      const success = await refetchTemperature(showLoading);
      return success;
    },
    [refetchTemperature]
  );

  const retryConnection = useCallback(() => {
    setConnectionError(null);
    fetchTemperatureAndChartData(true);
    fetchParameters();
  }, [fetchTemperatureAndChartData, fetchParameters]);

  const value: CleverCoffeeContextValue = {
    parameters,
    parametersBySection,
    loadingParams,
    errorParams,
    updateParameter,
    saveParameters,
    fetchParameters,
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
    currentTempData,
    isLoadingTemp,
    temperatureError,
    fetchTemperatureAndChartData,
    retryConnection,
    togglePid,
    toggleSteam,
    toggleBackflush,
    toggleTareScale,
    toggleScaleCalibration,
    wakeFromStandby,
  };

  return (
    <CleverCoffeeContext.Provider value={value}>
      {children}
    </CleverCoffeeContext.Provider>
  );
};

export type { CleverCoffeeContextValue };