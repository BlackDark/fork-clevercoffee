import { useState, useEffect, useCallback, useMemo } from "react";
import { parameterGroups } from "@/lib/parameter-groups";

// Import the complete parameter definitions
import {
  ensureCompleteParameters,
  createParameterWithDefaults,
} from "@/lib/parameter-metadata";
import type { Parameter as CompleteParameter } from "@/lib/parameter-types";

interface UseEnhancedParametersReturn {
  parameters: CompleteParameter[];
  visibleParameters: CompleteParameter[];
  parametersBySection: Record<string, CompleteParameter[]>;
  groupedParameters: Record<string, CompleteParameter[]>;
  loading: boolean;
  error: string | null;
  updateParameter: (name: string, value: string | number | boolean) => void;
  saveParameters: () => Promise<boolean>;
  refreshParameters: () => Promise<void>;
  getParameter: (name: string) => CompleteParameter | undefined;
  isPostingForm: boolean;
  showPostSucceeded: boolean;
}

export function useEnhancedParameters(
  filter?: string
): UseEnhancedParametersReturn {
  const [serverParameters, setServerParameters] = useState<CompleteParameter[]>(
    []
  );
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isPostingForm, setIsPostingForm] = useState(false);
  const [showPostSucceeded, setShowPostSucceeded] = useState(false);

  // Merge server parameters with complete definitions
  const parameters = useMemo(() => {
    return ensureCompleteParameters(serverParameters);
  }, [serverParameters]);

  // All parameters are visible - we handle disabled state through requiredParameters
  const visibleParameters = useMemo(() => {
    return parameters;
  }, [parameters]);

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

  // Fetch parameters from server
  const fetchParameters = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);

      let url = "/api/parameters";
      if (filter) {
        url += `?filter=${filter}`;
      }

      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      const data = await response.json();
      setServerParameters(
        data.sort((a: CompleteParameter, b: CompleteParameter) =>
          a.name.localeCompare(b.name)
        )
      );
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to fetch parameters"
      );
    } finally {
      setLoading(false);
    }
  }, [filter]);

  // Initial load
  useEffect(() => {
    fetchParameters();
  }, [fetchParameters]);

  // Update a parameter value locally
  const updateParameter = useCallback(
    (name: string, value: string | number | boolean) => {
      setServerParameters((prev) => {
        const existingIndex = prev.findIndex((p) => p.name === name);
        if (existingIndex >= 0) {
          // Update existing parameter
          const updated = [...prev];
          updated[existingIndex] = { ...updated[existingIndex], value };
          return updated;
        } else {
          // Add new parameter (for parameters not sent by server initially)
          const completeParam = createParameterWithDefaults(name);
          if (completeParam) {
            return [...prev, { ...completeParam, value }];
          }
          return prev;
        }
      });
    },
    []
  );

  // Save parameters to backend
  const saveParameters = useCallback(async (): Promise<boolean> => {
    try {
      setError(null);
      setIsPostingForm(true);

      // Only save visible parameters
      const paramsToSave = visibleParameters.filter(
        (param: CompleteParameter) =>
          // Don't save read-only parameters
          param.name !== "TEMP" && param.name !== "VERSION"
      );

      // Build form data
      const formData = new URLSearchParams();
      paramsToSave.forEach((param: CompleteParameter) => {
        formData.append(param.name, String(param.value));
      });

      const response = await fetch("/api/parameters", {
        method: "POST",
        headers: {
          "Content-Type": "application/x-www-form-urlencoded",
        },
        body: formData.toString(),
      });

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      // Show success state
      setShowPostSucceeded(true);
      setTimeout(() => setShowPostSucceeded(false), 2000);

      // Refresh parameters after save to get updated conditions
      await fetchParameters();
      return true;
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to save parameters"
      );
      return false;
    } finally {
      setIsPostingForm(false);
    }
  }, [visibleParameters, fetchParameters]);

  // Refresh parameters from backend
  const refreshParameters = useCallback(async () => {
    await fetchParameters();
  }, [fetchParameters]);

  // Get a specific parameter by name
  const getParameter = useCallback(
    (name: string): CompleteParameter | undefined => {
      return parameters.find((param: CompleteParameter) => param.name === name);
    },
    [parameters]
  );

  return {
    parameters,
    visibleParameters,
    parametersBySection,
    groupedParameters,
    loading,
    error,
    updateParameter,
    saveParameters,
    refreshParameters,
    getParameter,
    isPostingForm,
    showPostSucceeded,
  };
}
