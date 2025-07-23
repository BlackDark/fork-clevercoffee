import { writable, derived, get } from "svelte/store";
import { apiFetch, SERVER_BASE_URL } from "../api-config";
import type { Parameter } from "../parameter-types";
import { groupParametersBySection } from "../parameter-utils";
import { ensureCompleteParameters } from "../parameter-metadata";
import type { HistoryData, TemperatureData } from "../../types/api";

// Parameters store
export const parameters = writable<Parameter[]>([]);
export const loadingParams = writable<boolean>(true);
export const errorParams = writable<string | null>(null);

// Chart data stores
export const tempData = writable<{
  tempDates: Date[];
  curTempVals: number[];
  targetTempVals: number[];
}>({ tempDates: [], curTempVals: [], targetTempVals: [] });

export const heaterData = writable<{
  heaterDates: Date[];
  heaterPowerVals: number[];
}>({ heaterDates: [], heaterPowerVals: [] });

export const chartError = writable<string | null>(null);
export const isHistoryLoaded = writable<boolean>(false);

// Health stores
export const isOnline = writable<boolean>(true);
export const lastHealthCheck = writable<Date | null>(null);
export const connectionError = writable<string | null>(null);

// Temperature stores
export const currentTempData = writable<TemperatureData | null>(null);
export const isLoadingTemp = writable<boolean>(true);
export const temperatureError = writable<string | null>(null);

// Derived store for parameters by section
export const parametersBySection = derived(parameters, ($parameters) =>
  groupParametersBySection($parameters)
);

// Helper functions
export function addTempData(data: {
  currentTemp?: number;
  targetTemp?: number;
}) {
  tempData.update((prev) => {
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
}

export function addHeaterData(data: { heaterPower?: number }) {
  heaterData.update((prev) => {
    if (data.heaterPower !== undefined) {
      const now = new Date();
      return {
        heaterDates: [...prev.heaterDates, now],
        heaterPowerVals: [...prev.heaterPowerVals, data.heaterPower],
      };
    }
    return prev;
  });
}

// API functions
export async function fetchParameters(refresh = true) {
  try {
    if (refresh) {
      loadingParams.set(true);
      errorParams.set(null);
    }

    const response = await apiFetch("/parameters?filter=all");
    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);

    const data = await response.json();
    // Merge with metadata/defaults
    parameters.set(
      ensureCompleteParameters(data).sort((a, b) =>
        a.name.localeCompare(b.name)
      )
    );
    errorParams.set(null);
  } catch (err) {
    errorParams.set(
      err instanceof Error ? err.message : "Failed to fetch parameters"
    );
  } finally {
    loadingParams.set(false);
  }
}

export async function fetchHistoryData(): Promise<boolean> {
  try {
    chartError.set(null);
    const response = await apiFetch("/history");
    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);

    const historyData: HistoryData = await response.json();

    console.log("[fetchHistoryData] historyData:", historyData);

    const dates: Date[] = [];

    for (let i = historyData.heaterPowers.length; i > 0; i--) {
      const date = new Date();
      date.setSeconds(date.getSeconds() - 3 * i);
      dates.push(date);
    }

    tempData.set({
      tempDates: dates,
      curTempVals: historyData.currentTemps,
      targetTempVals: historyData.targetTemps,
    });

    heaterData.set({
      heaterDates: dates,
      heaterPowerVals: historyData.heaterPowers,
    });

    isHistoryLoaded.set(true);
    return true;
  } catch (err) {
    chartError.set("Failed to load chart history data");
    console.error("[fetchHistoryData] error:", err);
    isHistoryLoaded.set(false);
    return false;
  }
}

export function updateParameter(
  name: string,
  value: string | number | boolean
) {
  parameters.update((prev) =>
    prev.map((param) => (param.name === name ? { ...param, value } : param))
  );
}

export async function saveParameters(): Promise<boolean> {
  try {
    errorParams.set(null);
    const formData = new URLSearchParams();
    get(parameters).forEach((param) => {
      formData.append(param.name, String(param.value));
    });

    const response = await apiFetch("/parameters", {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: formData.toString(),
    });

    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
    await fetchParameters();
    return true;
  } catch (err) {
    errorParams.set(
      err instanceof Error ? err.message : "Failed to save parameters"
    );
    return false;
  }
}

export function getParameter(name: string): Parameter | undefined {
  return get(parameters).find((param) => param.name === name);
}

export async function checkHealth(): Promise<boolean> {
  try {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 3000);

    const response = await apiFetch("/health", {
      method: "GET",
      signal: controller.signal,
    });

    clearTimeout(timeoutId);
    const isHealthy = response.ok;
    isOnline.set(isHealthy);
    lastHealthCheck.set(new Date());
    connectionError.set(isHealthy ? null : "Service unavailable");
    return isHealthy;
  } catch {
    isOnline.set(false);
    lastHealthCheck.set(new Date());
    connectionError.set("Connection failed");
    return false;
  }
}

export async function fetchTemperatureAndChartData(
  showLoading = false
): Promise<boolean> {
  try {
    temperatureError.set(null);
    if (showLoading) {
      isLoadingTemp.set(true);
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 5000);

    const response = await apiFetch("/temperatures", {
      signal: controller.signal,
    });

    clearTimeout(timeoutId);
    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);

    const tempDataResponse: TemperatureData = await response.json();

    currentTempData.set(tempDataResponse);
    temperatureError.set(null);
    isLoadingTemp.set(false);

    // Only append if history has loaded
    if (get(isHistoryLoaded)) {
      addTempData({
        currentTemp: tempDataResponse.currentTemp,
        targetTemp: tempDataResponse.targetTemp,
      });

      addHeaterData({ heaterPower: tempDataResponse.heaterPower });
    }
    return true;
  } catch (err) {
    currentTempData.set(null);
    temperatureError.set("Temperature sensor offline");
    if (showLoading) {
      isLoadingTemp.set(false);
    }
    console.error("[fetchTemperatureAndChartData] error:", err);
    return false;
  }
}

// Event source connection
let eventSource: EventSource | null = null;
let retryTimeout: number | null = null;

export function connectEventSource() {
  const startEventSource = () => {
    if (eventSource) {
      eventSource.close();
    }

    eventSource = new EventSource(`${SERVER_BASE_URL}/events`);

    eventSource.onopen = () => {
      isOnline.set(true);
      connectionError.set(null);
      temperatureError.set(null);
    };

    eventSource.addEventListener("new_temps", (e) => {
      try {
        const tempDataResponse: TemperatureData = JSON.parse(e.data);
        currentTempData.set(tempDataResponse);
        temperatureError.set(null);
        isLoadingTemp.set(false);

        if (get(isHistoryLoaded)) {
          addTempData({
            currentTemp: tempDataResponse.currentTemp,
            targetTemp: tempDataResponse.targetTemp,
          });

          addHeaterData({ heaterPower: tempDataResponse.heaterPower });
        }
      } catch (err: unknown) {
        console.log("[EventSource] Error parsing temperature event:", err);
        temperatureError.set("Failed to parse temperature event");
        currentTempData.set(null);
      }
    });

    eventSource.addEventListener("weight", (e) => {
      try {
        console.log(e.data);
      } catch (err: unknown) {
        console.log("[EventSource] Error parsing weight event:", err);
      }
    });

    eventSource.onerror = () => {
      connectionError.set("Lost connection to event source");
      isOnline.set(false);
      temperatureError.set("Lost connection");
      currentTempData.set(null);

      if (eventSource) {
        eventSource.close();
        eventSource = null;
      }

      // Retry after 3 seconds
      if (retryTimeout) {
        clearTimeout(retryTimeout);
      }
      retryTimeout = window.setTimeout(startEventSource, 3000);
    };
  };

  startEventSource();

  return {
    disconnect: () => {
      if (eventSource) {
        eventSource.close();
        eventSource = null;
      }
      if (retryTimeout) {
        clearTimeout(retryTimeout);
        retryTimeout = null;
      }
    },
  };
}

export function retryConnection() {
  connectionError.set(null);
  fetchTemperatureAndChartData(true);
  fetchParameters();
  connectEventSource();
}

// Dedicated toggle functions
export async function togglePid(): Promise<boolean> {
  try {
    const response = await apiFetch("/pid", { method: "POST" });
    if (response.ok) {
      await fetchParameters(false);
      return true;
    }
    return false;
  } catch {
    return false;
  }
}

export async function toggleSteam(): Promise<boolean> {
  try {
    const response = await apiFetch("/steam", { method: "POST" });
    if (response.ok) {
      await fetchParameters(false);
      return true;
    }
    return false;
  } catch {
    return false;
  }
}

export async function toggleBackflush(): Promise<boolean> {
  try {
    const response = await apiFetch("/backflush", { method: "POST" });
    if (response.ok) {
      await fetchParameters(false);
      return true;
    }
    return false;
  } catch {
    return false;
  }
}

export async function toggleTareScale(): Promise<boolean> {
  try {
    const response = await apiFetch("/scale/tare", { method: "POST" });
    if (response.ok) {
      await fetchParameters(false);
      return true;
    }
    return false;
  } catch {
    return false;
  }
}

export async function toggleScaleCalibration(): Promise<boolean> {
  try {
    const response = await apiFetch("/scale/calibration", { method: "POST" });
    if (response.ok) {
      await fetchParameters(false);
      return true;
    }
    return false;
  } catch {
    return false;
  }
}

// Initialize data on import
fetchParameters();
