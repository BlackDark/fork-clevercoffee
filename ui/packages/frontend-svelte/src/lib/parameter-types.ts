// Unified parameter type definitions
// This file consolidates all parameter-related types and interfaces

export const ParameterTypes = {
  INTEGER: 0,
  UINT8: 1,
  DOUBLE: 2,
  FLOAT: 3,
  STRING: 4,
  ENUM: 5,
} as const;

export type ParameterType =
  (typeof ParameterTypes)[keyof typeof ParameterTypes];

export interface ParameterOption {
  value: number;
  label: string;
}

// Server parameter type - minimal data from backend
export interface ServerParameter {
  type: ParameterType;
  name: string;
  value: string | number | boolean;
  min: number;
  max: number;
  options?: ParameterOption[];
}

// UI parameter metadata - everything needed for display
export interface ParameterMetadata {
  type: ParameterType;
  min: number;
  max: number;
  defaultValue: string | number | boolean;
  options?: ParameterOption[];
  requiredParameters?: Record<string, string | number | boolean>;
}

// Complete parameter for UI use (server data + metadata)
export interface Parameter extends ServerParameter {
  requiredParameters?: Record<string, string | number | boolean>;
}

/**
 * Helper functions for parameter types
 */
export const isParameterBoolean = (
  param: Parameter | ParameterMetadata
): boolean => {
  return (
    param.type === ParameterTypes.UINT8 && param.min === 0 && param.max === 1
  );
};

export const isParameterEnum = (
  param: Parameter | ParameterMetadata
): boolean => {
  return param.type === ParameterTypes.ENUM && !!param.options;
};

export const isParameterString = (
  param: Parameter | ParameterMetadata
): boolean => {
  return param.type === ParameterTypes.STRING;
};

export const isParameterNumeric = (
  param: Parameter | ParameterMetadata
): boolean => {
  return (
    [
      ParameterTypes.INTEGER,
      ParameterTypes.DOUBLE,
      ParameterTypes.FLOAT,
    ] as ParameterType[]
  ).includes(param.type);
};
