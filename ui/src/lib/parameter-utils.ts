// Parameter management utilities
import { parameterHelpTexts } from "./parameter-help-texts";
import { parameterGroups } from "./parameter-groups";

export interface Parameter {
  type: number;
  name: string;
  displayName: string;
  section: number;
  position: number;
  hasHelpText: boolean;
  show: boolean;
  value: any;
  min: number;
  max: number;
  options?: Array<{ value: number; label: string }>;
  conditions?: {
    showWhen?: Record<string, any>;
    hideWhen?: Record<string, any>;
  };
}

/**
 * Parameter type constants matching backend
 */
export const ParameterTypes = {
  INTEGER: 0,
  UINT8: 1,
  DOUBLE: 2,
  FLOAT: 3,
  STRING: 4,
  ENUM: 5,
} as const;

/**
 * Evaluates parameter visibility conditions
 */
export function shouldShowParameter(
  parameter: Parameter,
  allParameters: Parameter[]
): boolean {
  if (!parameter.conditions) {
    return parameter.show;
  }

  const parameterMap = new Map(allParameters.map((p) => [p.name, p.value]));

  // Check showWhen conditions
  if (parameter.conditions.showWhen) {
    for (const [paramName, expectedValue] of Object.entries(
      parameter.conditions.showWhen
    )) {
      const actualValue = parameterMap.get(paramName);
      if (actualValue !== expectedValue) {
        return false;
      }
    }
  }

  // Check hideWhen conditions
  if (parameter.conditions.hideWhen) {
    for (const [paramName, expectedValue] of Object.entries(
      parameter.conditions.hideWhen
    )) {
      const actualValue = parameterMap.get(paramName);
      if (actualValue === expectedValue) {
        return false;
      }
    }
  }

  return parameter.show;
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

  return visibleParameters.reduce((acc, param) => {
    if (!acc[param.section]) {
      acc[param.section] = [];
    }
    acc[param.section].push(param);
    return acc;
  }, {} as Record<number, Parameter[]>);
}

/**
 * Gets help text for a parameter
 */
export function getParameterHelpText(parameterName: string): string {
  return parameterHelpTexts[parameterName] || "";
}

/**
 * Determines if parameter is boolean (checkbox)
 */
export function isBoolean(param: Parameter): boolean {
  return (
    param.type === ParameterTypes.UINT8 && param.min === 0 && param.max === 1
  );
}

/**
 * Gets HTML input type for parameter
 */
export function getInputType(param: Parameter): string {
  switch (param.type) {
    case ParameterTypes.INTEGER:
    case ParameterTypes.UINT8:
    case ParameterTypes.DOUBLE:
    case ParameterTypes.FLOAT:
      return "number";
    case ParameterTypes.STRING:
      return param.name.toLowerCase().includes("password")
        ? "password"
        : "text";
    case ParameterTypes.ENUM:
      return "select";
    default:
      return "text";
  }
}

/**
 * Gets step value for number inputs
 */
export function getNumberStep(param: Parameter): string {
  switch (param.type) {
    case ParameterTypes.INTEGER:
    case ParameterTypes.UINT8:
      return "1";
    case ParameterTypes.DOUBLE:
    case ParameterTypes.FLOAT:
      return "0.01";
    default:
      return "1";
  }
}

/**
 * Section names mapping
 */
export const sectionNames: Record<number, string> = {
  0: "PID Parameters",
  1: "Temperature",
  2: "Brew PID Parameters",
  3: "Brew Control",
  4: "Scale Parameters",
  5: "Display Settings",
  6: "Maintenance",
  7: "Power Settings",
  8: "MQTT Settings",
  9: "System Settings",
  10: "Other",
  11: "OLED Display",
  12: "Relays",
  13: "Switches",
  14: "LEDs",
  15: "Sensors",
};

/**
 * Gets section display name
 */
export function getSectionName(sectionId: number): string {
  return sectionNames[sectionId] || "Unknown Section";
}

/**
 * Validates parameter value against constraints
 */
export function validateParameterValue(param: Parameter, value: any): boolean {
  switch (param.type) {
    case ParameterTypes.INTEGER:
    case ParameterTypes.UINT8:
    case ParameterTypes.DOUBLE:
    case ParameterTypes.FLOAT:
      const numValue = Number(value);
      return !isNaN(numValue) && numValue >= param.min && numValue <= param.max;

    case ParameterTypes.STRING:
      const strValue = String(value);
      return strValue.length <= param.max;

    case ParameterTypes.ENUM:
      const enumValue = Number(value);
      return (
        !isNaN(enumValue) && enumValue >= param.min && enumValue <= param.max
      );

    default:
      return true;
  }
}

/**
 * Formats parameter value for display
 */
export function formatParameterValue(param: Parameter): string {
  switch (param.type) {
    case ParameterTypes.DOUBLE:
    case ParameterTypes.FLOAT:
      return Number(param.value).toFixed(2);
    case ParameterTypes.ENUM:
      const option = param.options?.find((opt) => opt.value === param.value);
      return option?.label || String(param.value);
    default:
      return String(param.value);
  }
}
