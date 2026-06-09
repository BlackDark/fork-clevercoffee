import type React from "react";
import { useCallback, useEffect, useState } from "react";
import { CleverCoffeeContext } from "@/context/useCleverCoffee";
import { useMachineToggles } from "@/hooks/useMachineToggles";
import { useParametersApi } from "@/hooks/useParametersApi";
import { useTemperatureStream } from "@/hooks/useTemperatureStream";
import { apiFetch } from "@/lib/api-config";
import { API_ROUTES } from "@/lib/routes";
import type { HistoryData, MachineStatus, TemperatureData } from "@/types/api";

interface CleverCoffeeContextValue {
  parameters: ReturnType<typeof useParametersApi>["parameters"];
  parametersBySection: ReturnType<
    typeof useParametersApi
  >["parametersBySection"];
  loadingParams: ReturnType<typeof useParametersApi>["loading"];
  errorParams: ReturnType<typeof useParametersApi>["error"];
  updateParameter: ReturnType<typeof useParametersApi>["updateParameter"];
  saveParameters: ReturnType<typeof useParametersApi>["saveParameters"];
  fetchParameters: ReturnType<typeof useParametersApi>["refetch"];
  getParameter: ReturnType<typeof useParametersApi>["getParameter"];

  tempData: {
    tempDates: Date[];
    curTempVals: number[];
    targetTempVals: number[];
  };
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
  toggleScaleCalibration: ReturnType<
    typeof useMachineToggles
  >["toggleScaleCalibration"];
  wakeFromStandby: ReturnType<typeof useMachineToggles>["wakeFromStandby"];
  sleepFromStandby: ReturnType<typeof useMachineToggles>["sleepFromStandby"];

  machineStatus: MachineStatus | null;
  machineStatusLoading: boolean;
  refetchMachineStatus: () => Promise<boolean>;
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
  const [machineStatus, setMachineStatus] = useState<MachineStatus | null>(
    null,
  );
  const [machineStatusLoading, setMachineStatusLoading] = useState(true);

  const refetchMachineStatus = useCallback(async (): Promise<boolean> => {
    try {
      const response = await apiFetch(API_ROUTES.STATUS, {
        signal: AbortSignal.timeout(5000),
      });
      if (response.ok) {
        setMachineStatus((await response.json()) as MachineStatus);
        return true;
      }
      setMachineStatus(null);
      return false;
    } catch {
      setMachineStatus(null);
      return false;
    } finally {
      setMachineStatusLoading(false);
    }
  }, []);

  useEffect(() => {
    let mounted = true;

    const poll = async () => {
      if (!mounted) return;
      await refetchMachineStatus();
    };

    poll();
    const interval = setInterval(poll, 10000);
    return () => {
      mounted = false;
      clearInterval(interval);
    };
  }, [refetchMachineStatus]);

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

  const addTempData = useCallback(
    (data: { currentTemp?: number; targetTemp?: number }) => {
      if (data.currentTemp != null && data.targetTemp != null) {
        const { currentTemp, targetTemp } = data;
        const now = new Date();
        setTempData((prev) => ({
          tempDates: [...prev.tempDates, now],
          curTempVals: [...prev.curTempVals, currentTemp],
          targetTempVals: [...prev.targetTempVals, targetTemp],
        }));
      }
    },
    [],
  );

  const addHeaterData = useCallback((data: { heaterPower?: number }) => {
    if (data.heaterPower !== undefined) {
      const { heaterPower } = data;
      const now = new Date();
      setHeaterData((prev) => ({
        heaterDates: [...prev.heaterDates, now],
        heaterPowerVals: [...prev.heaterPowerVals, heaterPower],
      }));
    }
  }, []);

  const handleLiveChartData = useCallback(
    (data: TemperatureData) => {
      addTempData({
        currentTemp: data.currentTemp,
        targetTemp: data.targetTemp,
      });
      addHeaterData({ heaterPower: data.heaterPower });
    },
    [addTempData, addHeaterData],
  );

  const {
    temperatureData: currentTempData,
    isLoading: isLoadingTemp,
    error: temperatureError,
    refetch: refetchTemperature,
  } = useTemperatureStream(isHistoryLoaded, handleLiveChartData);

  const {
    togglePid: rawTogglePid,
    toggleSteam: rawToggleSteam,
    toggleBackflush: rawToggleBackflush,
    toggleTareScale: rawToggleTareScale,
    toggleScaleCalibration: rawToggleScaleCalibration,
    wakeFromStandby: rawWakeFromStandby,
    sleepFromStandby: rawSleepFromStandby,
  } = useMachineToggles();

  const togglePid = useCallback(async () => {
    const success = await rawTogglePid();
    if (success) {
      fetchParameters(false);
    }
    return success;
  }, [rawTogglePid, fetchParameters]);
  const toggleSteam = useCallback(async () => {
    const success = await rawToggleSteam();
    if (success) {
      fetchParameters(false);
    }
    return success;
  }, [rawToggleSteam, fetchParameters]);
  const toggleBackflush = useCallback(async () => {
    const result = await rawToggleBackflush();
    if (result.success) {
      fetchParameters(false);
      await refetchMachineStatus();
    }
    return result;
  }, [rawToggleBackflush, fetchParameters, refetchMachineStatus]);
  const toggleTareScale = useCallback(async () => {
    const success = await rawToggleTareScale();
    if (success) {
      fetchParameters(false);
    }
    return success;
  }, [rawToggleTareScale, fetchParameters]);
  const toggleScaleCalibration = useCallback(async () => {
    const success = await rawToggleScaleCalibration();
    if (success) {
      fetchParameters(false);
    }
    return success;
  }, [rawToggleScaleCalibration, fetchParameters]);

  const wakeFromStandby = useCallback(async () => {
    const success = await rawWakeFromStandby();
    if (success) {
      await refetchMachineStatus();
    }
    return success;
  }, [rawWakeFromStandby, refetchMachineStatus]);

  const sleepFromStandby = useCallback(async () => {
    return await rawSleepFromStandby();
  }, [rawSleepFromStandby]);

  const fetchHistoryData = useCallback(async (): Promise<boolean> => {
    try {
      setChartError(null);
      const response = await apiFetch(API_ROUTES.HISTORY);
      if (!response.ok)
        throw new Error(`HTTP error! status: ${response.status}`);
      const historyData: HistoryData = await response.json();

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
    } catch {
      setChartError("Failed to load chart history data");
      setHistoryLoaded(false);
      return false;
    }
  }, []);

  const checkHealth = useCallback(async (): Promise<boolean> => {
    try {
      const response = await apiFetch(API_ROUTES.HEALTH, {
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

  const fetchTemperatureAndChartData = useCallback(
    async (showLoading = false): Promise<boolean> => {
      const success = await refetchTemperature(showLoading);
      return success;
    },
    [refetchTemperature],
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
    sleepFromStandby,
    machineStatus,
    machineStatusLoading,
    refetchMachineStatus,
  };

  return (
    <CleverCoffeeContext.Provider value={value}>
      {children}
    </CleverCoffeeContext.Provider>
  );
};

export type { CleverCoffeeContextValue };
