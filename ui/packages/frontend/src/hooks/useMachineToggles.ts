import { useCallback } from "react";
import { apiFetch } from "@/lib/api-config";
import { API_ROUTES } from "@/lib/routes";

interface UseMachineTogglesReturn {
  togglePid: () => Promise<boolean>;
  toggleSteam: () => Promise<boolean>;
  toggleBackflush: () => Promise<boolean>;
  toggleTareScale: () => Promise<boolean>;
  toggleScaleCalibration: () => Promise<boolean>;
}

export function useMachineToggles(): UseMachineTogglesReturn {
  const togglePid = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.PID, { method: "POST" });
      return response.ok;
    } catch {
      return false;
    }
  }, []);

  const toggleSteam = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.STEAM, { method: "POST" });
      return response.ok;
    } catch {
      return false;
    }
  }, []);

  const toggleBackflush = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.BACKFLUSH, { method: "POST" });
      return response.ok;
    } catch {
      return false;
    }
  }, []);

  const toggleTareScale = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.SCALE_TARE, { method: "POST" });
      return response.ok;
    } catch {
      return false;
    }
  }, []);

  const toggleScaleCalibration = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.SCALE_CALIBRATION, { method: "POST" });
      return response.ok;
    } catch {
      return false;
    }
  }, []);

  return {
    togglePid,
    toggleSteam,
    toggleBackflush,
    toggleTareScale,
    toggleScaleCalibration,
  };
}