import { useState, useEffect, useCallback, useRef } from "react";
import { apiFetch, apiJsonFetch } from "@/lib/api-config";
import type { HistoryData, ApiResponse } from "@/types/api";
import { useChartData } from "./use-chart-data";

export interface Parameter {
  name: string;
  displayName: string;
  value: unknown;
  hasHelpText: boolean;
  type: number;
  min: number;
  max: number;
  position: number;
}

export interface CleverCoffeeState {
  // Core data
  parameters: Parameter[];
  currentTemperature: string | null;
  parametersHelpTexts: Record<string, string>;

  // Loading states
  isLoadingParams: boolean;
  isLoadingTemp: boolean;
  isPostingForm: boolean;

  // UI states
  showPostSucceeded: boolean;

  // Error states
  connectionError: string | null;
  temperatureError: string | null;
  chartError: string | null;

  // Chart data
  tempData: ReturnType<typeof useChartData>["tempData"];
  heaterData: ReturnType<typeof useChartData>["heaterData"];
}

export interface CleverCoffeeActions {
  // Core actions
  fetchParameters: (filter?: string) => Promise<boolean>;
  fetchTemperatureAndChartData: (showLoading?: boolean) => Promise<boolean>;
  fetchHistoryData: () => Promise<boolean>;
  fetchHelpText: (paramName: string) => Promise<boolean>;

  // Parameter management
  updateParameterValue: (paramName: string, newValue: unknown) => void;
  postParameters: (parameterNames?: string[]) => Promise<boolean>;

  // Function toggles
  togglePid: () => Promise<boolean>;
  toggleSteamMode: () => Promise<boolean>;
  toggleBackflush: () => Promise<boolean>;

  // Scale actions
  tareScale: () => Promise<boolean>;
  calibrateScale: () => Promise<boolean>;

  // Error management
  clearConnectionError: () => void;
  clearTemperatureError: () => void;
  clearChartError: () => void;

  // Retry actions
  retryConnection: () => void;
}

export function useCleverCoffee(): CleverCoffeeState & CleverCoffeeActions {
  // State
  const [parameters, setParameters] = useState<Parameter[]>([]);
  const [parametersHelpTexts, setParametersHelpTexts] = useState<
    Record<string, string>
  >({});
  const [isPostingForm, setIsPostingForm] = useState(false);
  const [showPostSucceeded, setShowPostSucceeded] = useState(false);
  const [currentTemperature, setCurrentTemperature] = useState<string | null>(
    null
  );
  const [isLoadingTemp, setIsLoadingTemp] = useState(true);
  const [isLoadingParams, setIsLoadingParams] = useState(true);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [temperatureError, setTemperatureError] = useState<string | null>(null);
  const [chartError, setChartError] = useState<string | null>(null);

  // Chart data hook
  const { addTempData, addHeaterData, tempData, heaterData } = useChartData();

  // Refs for chart data functions to avoid dependency issues
  const addTempDataRef = useRef(addTempData);
  const addHeaterDataRef = useRef(addHeaterData);

  // Update refs when functions change
  addTempDataRef.current = addTempData;
  addHeaterDataRef.current = addHeaterData;

  // Ref to track current temperature for comparison without triggering re-renders
  const currentTempRef = useRef<string | null>(null);

  // Core API functions - unified temperature and chart data fetching
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

        if (!response.ok) {
          throw new Error(`HTTP error! status: ${response.status}`);
        }

        const liveData = await response.json();

        // Update current temperature if available in the response
        if (liveData.currentTemp !== undefined) {
          const newTemp = liveData.currentTemp.toString();
          // Only update if temperature has changed to prevent unnecessary re-renders
          if (currentTempRef.current !== newTemp) {
            setCurrentTemperature(newTemp);
            currentTempRef.current = newTemp;
          }
          setTemperatureError(null);
          // Always set loading to false on successful fetch to handle initial load
          setIsLoadingTemp(false);
        }

        // Add live chart data
        addTempDataRef.current(liveData, true);
        addHeaterDataRef.current(liveData, true);

        return true; // Success
      } catch (error: unknown) {
        console.error(
          "Error fetching temperature and chart data:",
          error as Error
        );
        setCurrentTemperature(null);
        currentTempRef.current = null;
        if (showLoading) {
          setIsLoadingTemp(false);
        }
        setTemperatureError("Temperature sensor offline");

        return false; // Failure
      }
    },
    [] // Empty dependency array since we use refs for chart functions
  );

  const fetchParameters = useCallback(
    async (filter: string = ""): Promise<boolean> => {
      setIsLoadingParams(true);
      let url = "/parameters";
      if (filter) {
        url += "?filter=" + filter;
      }
      try {
        const response = await apiFetch(url);
        if (!response.ok) {
          throw new Error(`HTTP error! status: ${response.status}`);
        }
        const data: Parameter[] = await response.json();
        setParameters(data.sort((a, b) => a.position - b.position));
        return true; // Success
      } catch (error) {
        console.error("Error fetching parameters:", error);
        return false; // Failure
      } finally {
        setIsLoadingParams(false);
      }
    },
    []
  );

  const fetchHistoryData = useCallback(async (): Promise<boolean> => {
    try {
      setChartError(null);
      const historyData = await apiJsonFetch<HistoryData>("/history");
      addTempDataRef.current(historyData, false);
      addHeaterDataRef.current(historyData, false);
      return true; // Success
    } catch (error: unknown) {
      console.error("Error fetching history data:", error as Error);
      setChartError("Failed to load chart history data");
      return false; // Failure
    }
  }, []); // Empty dependency array since we use refs for chart functions

  const fetchHelpText = useCallback(
    async (paramName: string): Promise<boolean> => {
      if (!(paramName in parametersHelpTexts)) {
        try {
          const response = await apiFetch(`/parameterHelp/?param=${paramName}`);
          const data = await response.json();
          setParametersHelpTexts((prev) => ({
            ...prev,
            [paramName]: data.helpText,
          }));
          return true; // Success
        } catch (error) {
          console.error("Error fetching help text:", error);
          return false; // Failure
        }
      }
      return true; // Already exists, consider it success
    },
    [parametersHelpTexts]
  );

  // Parameter management
  const updateParameterValue = useCallback(
    (paramName: string, newValue: unknown) => {
      setParameters((prev) =>
        prev.map((p) => (p.name === paramName ? { ...p, value: newValue } : p))
      );
    },
    []
  );

  const postParameters = useCallback(
    async (parameterNames?: string[]): Promise<boolean> => {
      setIsPostingForm(true);
      const formBody: string[] = [];

      parameters.forEach((param) => {
        // If specific parameters are provided, only post those, otherwise post all displayed parameters
        const shouldPost = parameterNames
          ? parameterNames.includes(param.name)
          : ["brew.setpoint"].includes(param.name); // Default to only brew.setpoint for home page

        if (shouldPost) {
          formBody.push(
            `${param.name}=${encodeURIComponent(param.value as string)}`
          );
        }
      });

      if (formBody.length === 0) {
        setIsPostingForm(false);
        return false; // No parameters to save
      }

      try {
        const response = await apiFetch("/parameters", {
          method: "POST",
          headers: {
            "Content-Type": "application/x-www-form-urlencoded",
          },
          body: formBody.join("&"),
        });
        if (!response.ok) {
          throw new Error(`HTTP error! status: ${response.status}`);
        }
        setShowPostSucceeded(true);
        setTimeout(() => setShowPostSucceeded(false), 3000);
        fetchParameters(); // Re-fetch to get updated values
        return true; // Success
      } catch (error) {
        console.error("Error saving parameters:", error);
        return false; // Failure
      } finally {
        setIsPostingForm(false);
      }
    },
    [parameters, fetchParameters]
  );

  // Function toggles
  const toggleFunction = useCallback(
    async (endpoint: string, paramName: string): Promise<boolean> => {
      const param = parameters.find((p) => p.name === paramName);
      if (!param) return false;

      try {
        const result = await apiJsonFetch<ApiResponse>(endpoint, {
          method: "POST",
        });
        if (result.success) {
          const newValue = param.value === 1 ? 0 : 1;
          updateParameterValue(paramName, newValue);
          return true; // Success
        } else {
          throw new Error("API returned success: false");
        }
      } catch (error) {
        console.error("Toggle failed:", error);
        return false; // Failure
      }
    },
    [parameters, updateParameterValue]
  );

  const togglePid = useCallback(
    () => toggleFunction("/pid", "pid.enabled"),
    [toggleFunction]
  );
  const toggleSteamMode = useCallback(
    () => toggleFunction("/steam", "STEAM_MODE"),
    [toggleFunction]
  );
  const toggleBackflush = useCallback(
    () => toggleFunction("/backflush", "BACKFLUSH_ON"),
    [toggleFunction]
  );

  // Scale actions
  const executeAction = useCallback(
    async (endpoint: string): Promise<boolean> => {
      try {
        const result = await apiJsonFetch<ApiResponse>(endpoint, {
          method: "POST",
        });

        if (result.success) {
          return true; // Success
        } else {
          throw new Error("API returned success: false");
        }
      } catch (error) {
        console.error("Error executing action:", error);
        return false; // Failure
      }
    },
    []
  );

  const tareScale = useCallback(
    (): Promise<boolean> => executeAction("/scale/tare"),
    [executeAction]
  );

  const calibrateScale = useCallback(async (): Promise<boolean> => {
    if (
      window.confirm("Are you sure you want to start the scale calibration?")
    ) {
      return await executeAction("/scale/calibration");
    }
    return false; // User cancelled
  }, [executeAction]);

  // Error management
  const clearConnectionError = useCallback(() => setConnectionError(null), []);
  const clearTemperatureError = useCallback(
    () => setTemperatureError(null),
    []
  );
  const clearChartError = useCallback(() => setChartError(null), []);

  const retryConnection = useCallback(() => {
    clearConnectionError();
    fetchTemperatureAndChartData(true); // Manual retry with UI feedback
    fetchParameters();
  }, [clearConnectionError, fetchTemperatureAndChartData, fetchParameters]);

  // Initialize data on mount
  useEffect(() => {
    const initializeData = async () => {
      await fetchParameters();
      await fetchHistoryData();
      await fetchTemperatureAndChartData(true); // Initial call with loading feedback
    };

    initializeData();
  }, [fetchParameters, fetchHistoryData, fetchTemperatureAndChartData]);

  return {
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
    fetchParameters,
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
    clearConnectionError,
    clearTemperatureError,
    clearChartError,
    retryConnection,
  };
}
