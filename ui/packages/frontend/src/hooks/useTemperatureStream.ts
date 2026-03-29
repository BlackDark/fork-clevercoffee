import { useState, useEffect, useCallback, useRef } from "react";
import { apiFetch, SERVER_BASE_URL } from "@/lib/api-config";
import { API_ROUTES } from "@/lib/routes";
import type { TemperatureData } from "@/types/api";

interface UseTemperatureStreamReturn {
  temperatureData: TemperatureData | null;
  isLoading: boolean;
  error: string | null;
  refetch: (showLoading?: boolean) => Promise<boolean>;
}

export function useTemperatureStream(historyLoaded: boolean): UseTemperatureStreamReturn {
  const [temperatureData, setTemperatureData] = useState<TemperatureData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const addTempRef = useRef<((data: { currentTemp?: number; targetTemp?: number }) => void) | null>(null);
  const addHeaterRef = useRef<((data: { heaterPower?: number }) => void) | null>(null);

  const refetch = useCallback(async (showLoading = false): Promise<boolean> => {
    try {
      setError(null);
      if (showLoading) setIsLoading(true);
      const response = await apiFetch(API_ROUTES.TEMPERATURES, {
        signal: AbortSignal.timeout(5000),
      });
      if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
      const data: TemperatureData = await response.json();
      setTemperatureData(data);
      setError(null);
      setIsLoading(false);
      return true;
    } catch {
      setTemperatureData(null);
      setError("Temperature sensor offline");
      setIsLoading(false);
      return false;
    }
  }, []);

  const connect = useCallback(() => {
    let events: EventSource | null = null;
    let retryTimeout: ReturnType<typeof setTimeout> | null = null;

    const startEventSource = () => {
      events = new EventSource(`${SERVER_BASE_URL}/events`);
      events.onopen = () => {
        setError(null);
      };

      events.addEventListener("new_temps", (e) => {
        try {
          const data: TemperatureData = JSON.parse(e.data);
          setTemperatureData(data);
          setError(null);
          setIsLoading(false);
          if (addTempRef.current && addHeaterRef.current && historyLoaded) {
            addTempRef.current({ currentTemp: data.currentTemp, targetTemp: data.targetTemp });
            addHeaterRef.current({ heaterPower: data.heaterPower });
          }
        } catch {
          setError("Failed to parse temperature event");
          setTemperatureData(null);
        }
      });

      events.addEventListener("weight", () => {});

      events.onerror = () => {
        setError("Lost connection");
        setTemperatureData(null);
        if (events) events.close();
        retryTimeout = setTimeout(startEventSource, 3000);
      };
    };

    startEventSource();
    return () => {
      if (events) events.close();
      if (retryTimeout) clearTimeout(retryTimeout);
    };
  }, [historyLoaded]);

  useEffect(() => {
    const cleanup = connect();
    return cleanup;
  }, [connect]);

  return { temperatureData, isLoading, error, refetch };
}