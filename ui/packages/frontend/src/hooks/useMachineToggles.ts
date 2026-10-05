import { useCallback } from "react";
import { apiFetch } from "@/lib/api-config";
import {
  type MachineToggleResult,
  parseToggleResponse,
} from "@/lib/machine-toggle-result";
import { API_ROUTES } from "@/lib/routes";

interface UseMachineTogglesReturn {
  togglePid: () => Promise<MachineToggleResult>;
  toggleSteam: () => Promise<MachineToggleResult>;
  toggleBackflush: () => Promise<MachineToggleResult>;
  toggleTareScale: () => Promise<boolean>;
  toggleScaleCalibration: () => Promise<boolean>;
  wakeFromStandby: () => Promise<boolean>;
  sleepFromStandby: () => Promise<boolean>;
}

export function useMachineToggles(): UseMachineTogglesReturn {
  // The three toggles read the state the **device** reports back, rather than
  // assuming the value flipped.
  //
  // The report was "the device switches correctly but the toggle stays active
  // until I refresh". These functions used to return a bare boolean, the page
  // showed a success toast, and nothing wrote the new value into the local
  // parameter list -- so the switch kept rendering the value it had before the
  // press until the next poll refetched it. `value` is what the device said; the
  // caller writes it into its own state.
  const togglePid = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.PID, { method: "POST" });
      return await parseToggleResponse(response, "pidEnabled");
    } catch {
      return { success: false };
    }
  }, []);

  const toggleSteam = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.STEAM, { method: "POST" });
      return await parseToggleResponse(response, "steamMode");
    } catch {
      return { success: false };
    }
  }, []);

  const toggleBackflush = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.BACKFLUSH, { method: "POST" });
      // The firmware answers `{"success":true,"backflushOn":<bool>}` —
      // `register_toggle`'s key for this route, not a name invented here.
      return await parseToggleResponse(response, "backflushOn");
    } catch {
      return { success: false };
    }
  }, []);

  const toggleTareScale = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.SCALE_TARE, {
        method: "POST",
      });
      return response.ok;
    } catch {
      return false;
    }
  }, []);

  const toggleScaleCalibration = useCallback(async () => {
    try {
      const response = await apiFetch(API_ROUTES.SCALE_CALIBRATION, {
        method: "POST",
      });
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
