import { useCallback } from "react";
import { apiFetch } from "@/lib/api-config";
import { parseToggleResponse, type MachineToggleResult } from "@/lib/machine-toggle-result";
import { API_ROUTES } from "@/lib/routes";

interface UseMachineTogglesReturn {
  togglePid: () => Promise<boolean>;
  toggleSteam: () => Promise<boolean>;
  toggleBackflush: () => Promise<MachineToggleResult>;
  toggleTareScale: () => Promise<boolean>;
  toggleScaleCalibration: () => Promise<boolean>;
  wakeFromStandby: () => Promise<boolean>;
  sleepFromStandby: () => Promise<boolean>;
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
      return await parseToggleResponse(response);
    } catch {
      return { success: false };
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

  const wakeFromStandby = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.WAKE, { method: "POST" });
      return response.ok;
    } catch {
      return false;
    }
  }, []);

  const sleepFromStandby = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.SLEEP, { method: "POST" });
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
    wakeFromStandby,
    sleepFromStandby,
  };
}