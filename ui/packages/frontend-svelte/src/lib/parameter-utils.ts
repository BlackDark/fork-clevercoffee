// Unified parameter management utilities
// This file consolidates all parameter processing logic

import type { Parameter } from "./parameter-types";
import {
  isParameterBoolean,
  isParameterEnum,
  isParameterString,
  isParameterNumeric,
} from "./parameter-types";
import { parameterHelpTexts } from "./parameter-help-texts";
import { getParameterLabel } from "./parameter-labels";
import { parameterGroups } from "./parameter-groups";

// Re-export types for convenience
export type {
  Parameter,
  ParameterType,
  ParameterOption,
} from "./parameter-types";

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
    if (actualValue !== expectedValue) {
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
    if (actualValue !== expectedValue) {
      const paramLabel = getParameterLabel(paramName) || paramName;
      const expectedLabel =
        expectedValue === 1 ? "enabled" : `set to ${expectedValue}`;
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
 * Legacy function for backward compatibility - now shows all parameters
 * Parameters are disabled in UI instead of hidden
 */
export function shouldShowParameter(): boolean {
  return true; // Always show parameters, but disable them if requirements not met
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

/**
 * Gets help text for a parameter
 */
export function getParameterHelpText(parameterName: string): string {
  return parameterHelpTexts[parameterName] || "";
}

/**
 * Gets HTML input type for parameter
 */
export function getInputType(param: Parameter): string {
  if (isParameterBoolean(param)) {
    return "checkbox";
  }
  if (isParameterEnum(param)) {
    return "select";
  }
  if (isParameterString(param)) {
    return "text";
  }
  if (isParameterNumeric(param)) {
    return "number";
  }
  return "text";
}

/**
 * Validates parameter value against constraints
 */
export function validateParameterValue(
  param: Parameter,
  value: string | number | boolean
): { isValid: boolean; error?: string } {
  if (isParameterBoolean(param)) {
    return {
      isValid: typeof value === "boolean" || value === 0 || value === 1,
    };
  }

  if (isParameterEnum(param) && param.options) {
    const numValue = typeof value === "number" ? value : Number(value);
    const isValid = param.options.some((option) => option.value === numValue);
    return {
      isValid,
      error: isValid ? undefined : "Invalid option selected",
    };
  }

  if (isParameterString(param)) {
    const strValue = String(value);
    const isValid = strValue.length <= param.max;
    return {
      isValid,
      error: isValid
        ? undefined
        : `Text too long (max ${param.max} characters)`,
    };
  }

  if (isParameterNumeric(param)) {
    const numValue = typeof value === "number" ? value : Number(value);
    if (isNaN(numValue)) {
      return { isValid: false, error: "Invalid number" };
    }
    const isValid = numValue >= param.min && numValue <= param.max;
    return {
      isValid,
      error: isValid
        ? undefined
        : `Value must be between ${param.min} and ${param.max}`,
    };
  }

  return { isValid: true };
}

/**
 * Formats parameter value for display
 */
export function formatParameterValue(param: Parameter): string {
  if (isParameterBoolean(param)) {
    return param.value ? "Yes" : "No";
  }

  if (isParameterEnum(param) && param.options) {
    const option = param.options.find((opt) => opt.value === param.value);
    return option?.label || String(param.value);
  }

  return String(param.value);
}

/**
 * Converts parameter value to appropriate type
 */
export function convertParameterValue(
  param: Parameter,
  value: string | number | boolean
): string | number | boolean {
  if (isParameterBoolean(param)) {
    if (typeof value === "boolean") return value;
    if (typeof value === "number") return value > 0;
    return value === "true" || value === "1";
  }

  if (isParameterNumeric(param) || isParameterEnum(param)) {
    return typeof value === "number" ? value : Number(value);
  }

  return String(value);
}

/**
 * Searches parameters by name or display name
 */
export function searchParameters(
  parameters: Parameter[],
  searchTerm: string
): Parameter[] {
  const term = searchTerm.toLowerCase();
  return parameters.filter(
    (param) =>
      param.name.toLowerCase().includes(term) ||
      getParameterLabel(param.name).toLowerCase().includes(term)
  );
}

/**
 * Sorts parameters by name alphabetically
 */
export function sortParameters(parameters: Parameter[]): Parameter[] {
  return [...parameters].sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * Gets parameters for a specific group
 */
export function getParametersForGroup(
  groupKey: string,
  allParameters: Parameter[]
): Parameter[] {
  const group = parameterGroups.find((g) => g.key === groupKey);
  if (!group) return [];

  const paramMap = new Map(allParameters.map((p) => [p.name, p]));
  return group.parameters
    .map((name) => paramMap.get(name))
    .filter((param): param is Parameter => param !== undefined);
}

/**
 * Determines if parameter is boolean (checkbox) - legacy alias
 */
export function isBoolean(param: Parameter): boolean {
  return isParameterBoolean(param);
}

/**
 * Gets the step value for number inputs
 */
export function getNumberStep(param: Parameter): string {
  if (param.type === 1 || param.type === 0) {
    // UINT8 or INTEGER
    return "1";
  }
  return "0.1"; // For DOUBLE and FLOAT
}

/**
 * Type guard to check if a value is a valid parameter value
 */
export function isValidParameterValue(
  value: unknown
): value is string | number | boolean {
  return (
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  );
}

/**
 * Get parameter by name from a list of parameters
 */
export function findParameterByName(
  parameters: Parameter[],
  name: string
): Parameter | undefined {
  return parameters.find((param) => param.name === name);
}

/**
 * Get all parameters that have validation errors
 */
export function getParametersWithErrors(
  parameters: Parameter[]
): Array<{ parameter: Parameter; error: string }> {
  const errors: Array<{ parameter: Parameter; error: string }> = [];

  parameters.forEach((param) => {
    const { isValid, error } = validateParameterValue(param, param.value);
    if (!isValid && error) {
      errors.push({ parameter: param, error });
    }
  });

  return errors;
}
