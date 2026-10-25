import {
  createMutation,
  createQuery,
  useQueryClient,
} from "@tanstack/svelte-query";
import { getContext, setContext } from "svelte";
import { apiJsonFetch } from "./api-config";
import {
  ensureCompleteParameters,
  getParameterMetadata,
  mapServerParameterToParameter,
} from "./parameter-metadata";
import { encodeParameterChanges } from "./parameter-save";
import type { UpdateParameter } from "./parameter-types";
import { API_ROUTES } from "./routes";
import {
  ApiResponseSchema,
  CommandResponseSchema,
  MachineStatusSchema,
  ServerParametersSchema,
} from "./schemas";

const KEY = Symbol("machine");
export function createMachine() {
  const client = useQueryClient();
  const parameters = createQuery(() => ({
    queryKey: ["parameters"],
    queryFn: async ({ signal }) => {
      const raw = ServerParametersSchema.parse(
        await apiJsonFetch(`${API_ROUTES.PARAMETERS}?filter=all`, { signal }),
      );
      const server = raw.map((p) => ({
        ...p,
        min: p.min ?? getParameterMetadata(p.name)?.min ?? 0,
        max: p.max ?? getParameterMetadata(p.name)?.max ?? 0,
      }));
      // Metadata supplies labels and dependencies, never writable rows absent from the device.
      const enriched = new Map(
        ensureCompleteParameters(server).map((p) => [p.name, p]),
      );
      return server.map(
        (p) => enriched.get(p.name) ?? mapServerParameterToParameter(p),
      );
    },
  }));
  const status = createQuery(() => ({
    queryKey: ["status"],
    queryFn: async ({ signal }) =>
      MachineStatusSchema.parse(
        await apiJsonFetch(API_ROUTES.STATUS, { signal }),
      ),
    refetchInterval: 10000,
  }));
  const save = createMutation(() => ({
    mutationFn: async (changes: UpdateParameter[]) => {
      const body = encodeParameterChanges(changes, parameters.data ?? []);
      const result = ApiResponseSchema.parse(
        await apiJsonFetch(API_ROUTES.PARAMETERS, {
          method: "POST",
          headers: { "Content-Type": "application/x-www-form-urlencoded" },
          body,
        }),
      );
      if (!result.success)
        throw new Error(result.message || "Saving parameters failed");
    },
    onSuccess: async () => {
      await Promise.all([
        client.invalidateQueries({ queryKey: ["parameters"] }),
        client.invalidateQueries({ queryKey: ["status"] }),
      ]);
    },
  }));
  const command = createMutation(() => ({
    mutationFn: async (route: string) => {
      const data = await apiJsonFetch(route, { method: "POST" });
      const result = CommandResponseSchema.parse(data);
      if ("success" in result && !result.success)
        throw new Error(result.message || "Command refused by device");
      if (
        route === API_ROUTES.STEAM ||
        route === API_ROUTES.BACKFLUSH ||
        route === API_ROUTES.PID
      ) {
        const key =
          route === API_ROUTES.STEAM
            ? "steamMode"
            : route === API_ROUTES.BACKFLUSH
              ? "backflushOn"
              : "pidEnabled";
        const value = (data as Record<string, unknown>)[key];
        if (typeof value === "boolean")
          client.setQueryData(
            ["status"],
            (
              previous:
                | ReturnType<typeof MachineStatusSchema.parse>
                | undefined,
            ) => ({ ...previous, [key]: value }),
          );
      }
    },
    onSuccess: async () => {
      await Promise.all([
        client.invalidateQueries({ queryKey: ["status"] }),
        client.invalidateQueries({ queryKey: ["parameters"] }),
      ]);
    },
  }));
  const draft = $state({
    edits: {} as Record<string, string | number | boolean>,
  });
  const machine = { parameters, status, save, command, draft };
  setContext(KEY, machine);
  return machine;
}
export function useMachine(): ReturnType<typeof createMachine> {
  return getContext(KEY);
}
