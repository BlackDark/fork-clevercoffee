import type { Parameter } from "./parameter-types";
import { getParameterLabel } from "./parameter-labels";
import { parameterGroups } from "./parameter-groups";

/**
 * Checks if a parameter's required parameters are met
 */
export function areRequiredParametersMet(
  parameter: Parameter,
  allParameters: Parameter[]
): boolean {
  if (!parameter.requiredParameters) {
    return true; // No requirements, always enabled
  }

  // Create a lookup map for parameter values
  const parameterValues = allParameters.reduce((acc, param) => {
    acc[param.name] = param.value;
    return acc;
  }, {} as Record<string, string | number | boolean>);

  // Check all required parameters
  for (const [paramName, expectedValue] of Object.entries(
    parameter.requiredParameters
  )) {
    const actualValue = parameterValues[paramName];

    // Handle boolean conversion: server might send 0/1 but we expect true/false
    let normalizedActualValue = actualValue;
    let normalizedExpectedValue = expectedValue;

    // Convert numeric boolean values to actual booleans for comparison
    if (typeof expectedValue === "boolean" && typeof actualValue === "number") {
      normalizedActualValue = actualValue === 1;
    } else if (
      typeof actualValue === "boolean" &&
      typeof expectedValue === "number"
    ) {
      normalizedExpectedValue = expectedValue === 1;
    }

    if (normalizedActualValue !== normalizedExpectedValue) {
      return false; // Required parameter not met
    }
  }

  return true; // All requirements met
}

/**
 * Gets a human-readable description of missing required parameters
 */
export function getMissingRequiredParametersMessage(
  parameter: Parameter,
  allParameters: Parameter[]
): string {
  if (!parameter.requiredParameters) {
    return "";
  }

  const parameterValues = allParameters.reduce((acc, param) => {
    acc[param.name] = param.value;
    return acc;
  }, {} as Record<string, string | number | boolean>);

  const missingRequirements: string[] = [];

  for (const [paramName, expectedValue] of Object.entries(
    parameter.requiredParameters
  )) {
    const actualValue = parameterValues[paramName];

    // Handle boolean conversion: server might send 0/1 but we expect true/false
    let normalizedActualValue = actualValue;
    let normalizedExpectedValue = expectedValue;

    // Convert numeric boolean values to actual booleans for comparison
    if (typeof expectedValue === "boolean" && typeof actualValue === "number") {
      normalizedActualValue = actualValue === 1;
    } else if (
      typeof actualValue === "boolean" &&
      typeof expectedValue === "number"
    ) {
      normalizedExpectedValue = expectedValue === 1;
    }

    if (normalizedActualValue !== normalizedExpectedValue) {
      const paramLabel = getParameterLabel(paramName) || paramName;
      const expectedLabel =
        expectedValue === true || expectedValue === 1
          ? "enabled"
          : `set to ${expectedValue}`;
      missingRequirements.push(`'${paramLabel}' must be ${expectedLabel}`);
    }
  }

  if (missingRequirements.length === 0) {
    return "";
  }

  if (missingRequirements.length === 1) {
    return `Enable this by setting: ${missingRequirements[0]}`;
  }

  return `Enable this by setting: ${missingRequirements.join(" and ")}`;
}

/**
 * Groups parameters by prefix (e.g., "pid", "brew", "hardware") with conditional filtering
 */
export function groupParametersByPrefix(
  parameters: Parameter[]
): Record<string, Parameter[]> {
  const visibleParameters = parameters.filter(() => true); // Show all parameters

  return visibleParameters.reduce((groups, param) => {
    const prefix = param.name.split(".")[0];
    if (!groups[prefix]) {
      groups[prefix] = [];
    }
    groups[prefix].push(param);
    return groups;
  }, {} as Record<string, Parameter[]>);
}

/**
 * Groups parameters by logical groups defined in parameter-groups.ts
 * This provides proper separation (e.g., scale parameters in behavior vs hardware)
 */
export function groupParametersBySection(
  parameters: Parameter[]
): Record<string, Parameter[]> {
  const visibleParameters = parameters.filter(() => true); // Show all parameters

  const groups: Record<string, Parameter[]> = {};

  // Create a lookup map for parameter names to groups
  const parameterToGroup = new Map<string, string>();
  for (const group of parameterGroups) {
    for (const paramName of group.parameters) {
      parameterToGroup.set(paramName, group.label);
    }
  }

  // Group parameters by their defined logical groups
  for (const param of visibleParameters) {
    const groupLabel = parameterToGroup.get(param.name);
    if (groupLabel) {
      if (!groups[groupLabel]) {
        groups[groupLabel] = [];
      }
      groups[groupLabel].push(param);
    } else {
      // Fallback to prefix-based grouping for parameters not in groups
      const prefix = param.name.split(".")[0];
      const fallbackLabel = `${prefix.charAt(0).toUpperCase()}${prefix.slice(
        1
      )} Parameters`;
      if (!groups[fallbackLabel]) {
        groups[fallbackLabel] = [];
      }
      groups[fallbackLabel].push(param);
    }
  }

  return groups;
}
