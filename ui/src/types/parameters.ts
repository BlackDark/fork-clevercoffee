export const ParameterTypes = {
  INTEGER: 0,
  BOOLEAN: 1,
  FLOAT: 2,
  STRING: 4,
  SELECT: 5,
} as const;

export type ParameterType =
  (typeof ParameterTypes)[keyof typeof ParameterTypes];

export type ParameterOption = {
  value: number;
  label: string;
};

export type Parameter = {
  type: ParameterType;
  name: string;
  value: number | string;
  min?: number;
  max?: number;
  options?: ParameterOption[];
  section?: number;
  position?: number;
};
