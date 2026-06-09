import { useCallback, useMemo, useState } from "react";
import { apiFetch } from "@/lib/api-config";
import { ensureCompleteParameters } from "@/lib/parameter-metadata";
import type { Parameter, UpdateParameter } from "@/lib/parameter-types";
import { groupParametersBySection } from "@/lib/parameter-utils";
import { API_ROUTES } from "@/lib/routes";

interface UseParametersApiReturn {
  parameters: Parameter[];
  parametersBySection: Record<string, Parameter[]>;
  loading: boolean;
  error: string | null;
  updateParameter: (name: string, value: string | number | boolean) => void;
  saveParameters: (updateParamaters?: UpdateParameter[]) => Promise<boolean>;
  refetch: (refresh?: boolean) => Promise<void>;
  getParameter: (name: string) => Parameter | undefined;
}

export function useParametersApi(): UseParametersApiReturn {
  const [parameters, setParameters] = useState<Parameter[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refetch = useCallback(async (refresh = true) => {
    try {
      if (refresh) {
        setLoading(true);
        setError(null);
      }
      const response = await apiFetch(`${API_ROUTES.PARAMETERS}?filter=all`);
      if (!response.ok)
        throw new Error(`HTTP error! status: ${response.status}`);
      const data = await response.json();
      setParameters(
        ensureCompleteParameters(data).sort((a, b) =>
          a.name.localeCompare(b.name),
        ),
      );
      setError(null);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to fetch parameters",
      );
    } finally {
      setLoading(false);
    }
  }, []);

  const updateParameter = useCallback(
    (name: string, value: string | number | boolean) => {
      setParameters((prev) =>
        prev.map((param) =>
          param.name === name ? { ...param, value } : param,
        ),
      );
    },
    [],
  );

  const saveParameters = useCallback(
    async (changedParams?: UpdateParameter[]): Promise<boolean> => {
      try {
        setError(null);
        const formData = new URLSearchParams();

        if (changedParams != null) {
          if (changedParams.length > 0) {
            changedParams.forEach((param) => {
              formData.append(param.name, String(param.value));
            });
          } else {
            return true;
          }
        } else {
          parameters.forEach((param) => {
            formData.append(param.name, String(param.value));
          });
        }
        const response = await apiFetch(API_ROUTES.PARAMETERS, {
          method: "POST",
          headers: { "Content-Type": "application/x-www-form-urlencoded" },
          body: formData.toString(),
        });
        if (!response.ok)
          throw new Error(`HTTP error! status: ${response.status}`);
        await refetch(false);
        return true;
      } catch (err) {
        setError(
          err instanceof Error ? err.message : "Failed to save parameters",
        );
        return false;
      }
    },
    [parameters, refetch],
  );

  const getParameter = useCallback(
    (name: string): Parameter | undefined => {
      return parameters.find((param) => param.name === name);
    },
    [parameters],
  );

  const parametersBySection = useMemo(
    () => groupParametersBySection(parameters),
    [parameters],
  );

  return {
    parameters,
    parametersBySection,
    loading,
    error,
    updateParameter,
    saveParameters,
    refetch,
    getParameter,
  };
}
