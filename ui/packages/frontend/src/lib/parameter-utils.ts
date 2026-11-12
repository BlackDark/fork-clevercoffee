import * as m from "@/paraglide/messages.js";
import { getParameterLabel } from "./parameter-text";
import type { Parameter } from "./parameter-types";

/**
 * Checks if a parameter's required parameters are met
 */
export function areRequiredParametersMet(
  parameter: Parameter,
  allParameters: Parameter[],
): boolean {
  if (!parameter.requiredParameters) {
    return true; // No requirements, always enabled
  }

  // Create a lookup map for parameter values
  const parameterValues = allParameters.reduce(
    (acc, param) => {
      acc[param.name] = param.value;
      return acc;
    },
    {} as Record<string, string | number | boolean>,
  );

  // Check all required parameters
  for (const [paramName, expectedValue] of Object.entries(
    parameter.requiredParameters,
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
  allParameters: Parameter[],
): string {
  if (!parameter.requiredParameters) {
    return "";
  }

  const parameterValues = allParameters.reduce(
    (acc, param) => {
      acc[param.name] = param.value;
      return acc;
    },
    {} as Record<string, string | number | boolean>,
  );

  const missingRequirements: string[] = [];

  for (const [paramName, expectedValue] of Object.entries(
    parameter.requiredParameters,
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
      missingRequirements.push(
        expectedValue === true || expectedValue === 1
          ? m.param_must_be_enabled({ name: paramLabel })
          : m.param_must_be_set_to({
              name: paramLabel,
              value: String(expectedValue),
            }),
      );
    }
  }

  if (missingRequirements.length === 0) {
    return "";
  }

  if (missingRequirements.length === 1) {
    return m.param_enable_by_setting({
      requirements: missingRequirements[0],
    });
  }

  return m.param_enable_by_setting({
    requirements: missingRequirements.join(m.param_and()),
  });
}
