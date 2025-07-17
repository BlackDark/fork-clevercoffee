export const ParameterTypes = {
  INTEGER: 0, // kInteger
  UINT8: 1, // kUInt8 (used for booleans when min=0, max=1)
  DOUBLE: 2, // kDouble
  FLOAT: 3, // kFloat
  STRING: 4, // kCString
  ENUM: 5, // kEnum (select dropdown)
} as const;

// Helper to check if a parameter is boolean
export const isParameterBoolean = (param: {
  type: number;
  min: number;
  max: number;
}) => {
  return (
    param.type === ParameterTypes.UINT8 && param.min === 0 && param.max === 1
  );
};

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
