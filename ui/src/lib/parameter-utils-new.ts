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
import { parameterGroups } from "./parameter-groups";

/**
 * Evaluates whether a parameter should be shown based on its conditions
 * and the current values of other parameters
 */
export function shouldShowParameter(
  parameter: Parameter,
  allParameters: Parameter[]
): boolean {
  if (!parameter.conditions) {
    return parameter.show !== false; // Default to true unless explicitly set to false
  }

  const { showWhen, hideWhen } = parameter.conditions;

  // Create a lookup map for parameter values
  const parameterValues = allParameters.reduce((acc, param) => {
    acc[param.name] = param.value;
    return acc;
  }, {} as Record<string, string | number | boolean>);

  // Check hideWhen conditions first (takes precedence)
  if (hideWhen) {
    for (const [paramName, expectedValue] of Object.entries(hideWhen)) {
      const actualValue = parameterValues[paramName];
      if (actualValue === expectedValue) {
        return false; // Hide the parameter
      }
    }
  }

  // Check showWhen conditions
  if (showWhen) {
    for (const [paramName, expectedValue] of Object.entries(showWhen)) {
      const actualValue = parameterValues[paramName];
      if (actualValue !== expectedValue) {
        return false; // Don't show the parameter
      }
    }
  }

  return parameter.show !== false; // Default to true
}

/**
 * Groups parameters by section with conditional filtering
 */
export function groupParametersBySection(
  parameters: Parameter[]
): Record<number, Parameter[]> {
  const visibleParameters = parameters.filter((param) =>
    shouldShowParameter(param, parameters)
  );

  return visibleParameters.reduce((groups, param) => {
    const section = param.section;
    if (!groups[section]) {
      groups[section] = [];
    }
    groups[section].push(param);
    return groups;
  }, {} as Record<number, Parameter[]>);
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
      (param.displayName && param.displayName.toLowerCase().includes(term))
  );
}

/**
 * Sorts parameters by section and position
 */
export function sortParameters(parameters: Parameter[]): Parameter[] {
  return [...parameters].sort((a, b) => {
    if (a.section !== b.section) {
      return a.section - b.section;
    }
    return a.position - b.position;
  });
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
