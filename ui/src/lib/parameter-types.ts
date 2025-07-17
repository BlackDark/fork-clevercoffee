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

export interface ParameterCondition {
  showWhen?: Record<string, string | number | boolean>;
  hideWhen?: Record<string, string | number | boolean>;
}

export interface Parameter {
  type: ParameterType;
  name: string;
  displayName?: string;
  section: number;
  position: number;
  hasHelpText?: boolean;
  show?: boolean;
  value: string | number | boolean;
  min: number;
  max: number;
  options?: ParameterOption[];
  conditions?: ParameterCondition;
}

/**
 * Helper functions for parameter types
 */
export const isParameterBoolean = (param: Parameter): boolean => {
  return (
    param.type === ParameterTypes.UINT8 && param.min === 0 && param.max === 1
  );
};

export const isParameterEnum = (param: Parameter): boolean => {
  return param.type === ParameterTypes.ENUM && !!param.options;
};

export const isParameterString = (param: Parameter): boolean => {
  return param.type === ParameterTypes.STRING;
};

export const isParameterNumeric = (param: Parameter): boolean => {
  return (
    [
      ParameterTypes.INTEGER,
      ParameterTypes.DOUBLE,
      ParameterTypes.FLOAT,
    ] as ParameterType[]
  ).includes(param.type);
};
