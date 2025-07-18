import { useMemo, useCallback } from "react";
import { parameterGroups } from "@/lib/parameter-groups";
import { ensureCompleteParameters } from "@/lib/parameter-metadata";
import type { Parameter as CompleteParameter } from "@/lib/parameter-types";
import { useCleverCoffee } from "@/context/useCleverCoffee";
import { apiFetch } from "@/lib/api-config";

interface UseEnhancedParametersReturn {
  parameters: CompleteParameter[];
  visibleParameters: CompleteParameter[];
  parametersBySection: Record<string, CompleteParameter[]>;
  groupedParameters: Record<string, CompleteParameter[]>;
  loading: boolean;
  error: string | null;
  updateParameter: (name: string, value: string | number | boolean) => void;
  saveParameters: (
    changedParams?: Record<string, string | number | boolean>
  ) => Promise<boolean>;
  refreshParameters: () => Promise<void>;
  getParameter: (name: string) => CompleteParameter | undefined;
}

export function useEnhancedParameters(
  filter?: string
): UseEnhancedParametersReturn {
  const ctx = useCleverCoffee();
  // Merge server parameters with metadata defaults
  const parameters = useMemo(
    () => ensureCompleteParameters(ctx.parameters),
    [ctx.parameters]
  );
  const visibleParameters = parameters;

  // Group parameters by prefix instead of section
  const parametersBySection = useMemo(() => {
    return visibleParameters.reduce(
      (acc: Record<string, CompleteParameter[]>, param: CompleteParameter) => {
        const prefix = param.name.split(".")[0];
        if (!acc[prefix]) {
          acc[prefix] = [];
        }
        acc[prefix].push(param);
        return acc;
      },
      {} as Record<string, CompleteParameter[]>
    );
  }, [visibleParameters]);

  // Map parameter group keys to categories
  const groupCategoryMap = useMemo(
    (): Record<string, string> => ({
      pidParameters: "behavior",
      temperatureControl: "behavior",
      brewPidSection: "behavior",
      brewControl: "behavior",
      scaleParameters: "behavior",
      displaySettings: "behavior",
      maintenance: "behavior",
      powerSettings: "behavior",
      mqttSettings: "system",
      systemSettings: "system",
      systemAuth: "system",
      runtimeControls: "system",
      oledDisplay: "hardware",
      relays: "hardware",
      switchesBrew: "hardware",
      switchesSteam: "hardware",
      switchesPower: "hardware",
      switchesHotWater: "hardware",
      ledsStatus: "hardware",
      ledsBrew: "hardware",
      ledsSteam: "hardware",
      sensorTemperature: "hardware",
      sensorPressure: "hardware",
      sensorWatertank: "hardware",
      sensorScale: "hardware",
    }),
    []
  );

  // Group parameters for display
  const groupedParameters = useMemo(() => {
    const selectedCategory = filter || "behavior";
    const filteredGroups = parameterGroups.filter(
      (group) => groupCategoryMap[group.key] === selectedCategory
    );
    const paramMap = Object.fromEntries(
      visibleParameters.map((p: CompleteParameter) => [p.name, p])
    );
    const result: Record<string, CompleteParameter[]> = {};
    filteredGroups.forEach((group) => {
      const groupParams = group.parameters
        .map((name) => paramMap[name])
        .filter(Boolean);
      if (groupParams.length > 0) {
        result[group.label] = groupParams;
      }
    });
    return result;
  }, [visibleParameters, filter, groupCategoryMap]);

  // Save parameters to backend (accepts changedParams map)
  const saveParameters = useCallback(
    async (
      changedParams?: Record<string, string | number | boolean>
    ): Promise<boolean> => {
      try {
        const formData = new URLSearchParams();
        if (changedParams) {
          Object.entries(changedParams).forEach(([name, value]) => {
            formData.append(name, String(value));
          });
        } else {
          parameters.forEach((param) => {
            formData.append(param.name, String(param.value));
          });
        }
        const response = await apiFetch("/parameters", {
          method: "POST",
          headers: { "Content-Type": "application/x-www-form-urlencoded" },
          body: formData.toString(),
        });
        if (!response.ok)
          throw new Error(`HTTP error! status: ${response.status}`);
        await ctx.refreshParameters();
        return true;
      } catch {
        return false;
      }
    },
    [parameters, ctx]
  );

  return {
    parameters,
    visibleParameters,
    parametersBySection,
    groupedParameters,
    loading: ctx.loadingParams,
    error: ctx.errorParams,
    updateParameter: ctx.updateParameter,
    saveParameters,
    refreshParameters: ctx.refreshParameters,
    getParameter: ctx.getParameter,
  };
}
