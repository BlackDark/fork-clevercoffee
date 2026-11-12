import * as m from "@/paraglide/messages.js";
import type { Parameter, UpdateParameter } from "./parameter-types";
export function encodeParameterChanges(
  changes: UpdateParameter[],
  parameters: Parameter[],
): URLSearchParams {
  const known = new Map(parameters.map((p) => [p.name, p]));
  for (const change of changes) {
    const parameter = known.get(change.name);
    if (!parameter)
      throw new Error(m.param_unknown_device_parameter({ name: change.name }));
    if (typeof change.value === "number") {
      if (!Number.isFinite(change.value))
        throw new Error(m.param_enter_finite_number({ name: change.name }));
      if (
        typeof parameter.min === "number" &&
        Number.isFinite(parameter.min) &&
        typeof parameter.max === "number" &&
        Number.isFinite(parameter.max) &&
        (change.value < parameter.min || change.value > parameter.max)
      )
        throw new Error(
          m.param_enter_value_between({
            name: change.name,
            min: parameter.min,
            max: parameter.max,
          }),
        );
    }
  }
  const body = new URLSearchParams(
    changes.map((p) => [p.name, String(p.value)]),
  );
  if (new TextEncoder().encode(body.toString()).length > 1024)
    throw new Error(m.param_too_many_changes());
  return body;
}
