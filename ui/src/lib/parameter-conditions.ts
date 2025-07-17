// Parameter condition evaluation utilities
export interface ParameterCondition {
  showWhen?: Record<string, any>;
  hideWhen?: Record<string, any>;
}

export interface Parameter {
  name: string;
  value: any;
  conditions?: ParameterCondition;
  show?: boolean;
  [key: string]: any;
}

/**
 * Evaluates whether a parameter should be shown based on its conditions
 * and the current values of other parameters
 */
export function evaluateParameterConditions(
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
  }, {} as Record<string, any>);

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

  return parameter.show !== false; // Default to true unless explicitly set to false
}

/**
 * Filters parameters based on their conditions and current parameter values
 */
export function filterParametersByConditions(
  parameters: Parameter[]
): Parameter[] {
  return parameters.filter((param) =>
    evaluateParameterConditions(param, parameters)
  );
}

/**
 * Groups parameters by section and applies conditional filtering
 */
export function groupParametersBySection(
  parameters: Parameter[]
): Record<number, Parameter[]> {
  const filteredParams = filterParametersByConditions(parameters);

  return filteredParams.reduce((acc, param) => {
    const section = param.section || 0;
    if (!acc[section]) {
      acc[section] = [];
    }
    acc[section].push(param);
    return acc;
  }, {} as Record<number, Parameter[]>);
}

/**
 * Section names mapping (matches backend ParameterSection enum)
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
 * Gets the display name for a section
 */
export function getSectionName(sectionId: number): string {
  return sectionNames[sectionId] || "Unknown Section";
}

/**
 * Determines if a parameter is a boolean type (checkbox)
 */
export function isBoolean(param: Parameter): boolean {
  return param.type === 1 && param.min === 0 && param.max === 1;
}

/**
 * Gets the appropriate input type for HTML input elements
 */
export function getInputType(param: Parameter): string {
  switch (param.type) {
    case 0: // integer
    case 1: // uint8
    case 2: // double
    case 3: // float
      return "number";
    case 4: // string
      return param.name.toLowerCase().includes("password")
        ? "password"
        : "text";
    case 5: // enum
      return "select";
    default:
      return "text";
  }
}

/**
 * Gets the step value for number inputs
 */
export function getNumberStep(param: Parameter): string {
  switch (param.type) {
    case 0: // integer
    case 1: // uint8
      return "1";
    case 2: // double
    case 3: // float
      return "0.01";
    default:
      return "1";
  }
}
