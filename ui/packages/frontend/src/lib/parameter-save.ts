import type { Parameter, UpdateParameter } from "./parameter-types";
export function encodeParameterChanges(
  changes: UpdateParameter[],
  parameters: Parameter[],
): URLSearchParams {
  const known = new Map(parameters.map((p) => [p.name, p]));
  for (const change of changes) {
    const parameter = known.get(change.name);
    if (!parameter) throw new Error(`Unknown device parameter: ${change.name}`);
    if (
      typeof change.value === "number" &&
      (!Number.isFinite(change.value) ||
        change.value < parameter.min ||
        change.value > parameter.max)
    )
      throw new Error(
        `${change.name}: enter a value between ${parameter.min} and ${parameter.max}.`,
      );
  }
  const body = new URLSearchParams(
    changes.map((p) => [p.name, String(p.value)]),
  );
  if (new TextEncoder().encode(body.toString()).length > 1024)
    throw new Error(
      "Too many changes for one request. Save fewer settings at a time (device limit: 1024 bytes).",
    );
  return body;
}
