import { useState, useEffect, useCallback } from "react";
import type { Parameter } from "../lib/parameter-types";
import { groupParametersBySection } from "../lib/parameter-utils";

interface UseParametersReturn {
  parameters: Parameter[];
  parametersBySection: Record<string, Parameter[]>;
  loading: boolean;
  error: string | null;
  updateParameter: (name: string, value: string | number | boolean) => void;
  saveParameters: () => Promise<boolean>;
  refreshParameters: () => Promise<void>;
  getParameter: (name: string) => Parameter | undefined;
}

export function useParameters(filter?: string): UseParametersReturn {
  const [parameters, setParameters] = useState<Parameter[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Fetch parameters from API
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
      setParameters(
        data.sort((a: Parameter, b: Parameter) => a.name.localeCompare(b.name))
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
      setParameters((prev) =>
        prev.map((param) => (param.name === name ? { ...param, value } : param))
      );
    },
    []
  );

  // Save parameters to backend
  const saveParameters = useCallback(async (): Promise<boolean> => {
    try {
      setError(null);

      // Filter visible parameters for saving
      const visibleParameters = parameters; // Show all parameters now

      // Build form data
      const formData = new URLSearchParams();
      visibleParameters.forEach((param) => {
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

      // Refresh parameters after save to get updated conditions
      await fetchParameters();
      return true;
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to save parameters"
      );
      return false;
    }
  }, [parameters, fetchParameters]);

  // Refresh parameters from backend
  const refreshParameters = useCallback(async () => {
    await fetchParameters();
  }, [fetchParameters]);

  // Get a specific parameter by name
  const getParameter = useCallback(
    (name: string): Parameter | undefined => {
      return parameters.find((param) => param.name === name);
    },
    [parameters]
  );

  // Group parameters by section with conditional filtering
  const parametersBySection = groupParametersBySection(parameters);

  return {
    parameters,
    parametersBySection,
    loading,
    error,
    updateParameter,
    saveParameters,
    refreshParameters,
    getParameter,
  };
}
