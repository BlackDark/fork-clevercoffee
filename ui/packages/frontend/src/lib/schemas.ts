import { z } from "zod/mini";

export const TemperatureDataSchema = z.object({
  currentTemp: z.number(),
  targetTemp: z.number(),
  heaterPower: z.number(),
});

export const HistoryDataSchema = z.object({
  data: z.optional(z.array(z.unknown())),
  currentTemps: z.array(z.number()),
  targetTemps: z.array(z.number()),
  heaterPowers: z.array(z.number()),
});

export const ParameterSchema = z.object({
  id: z.string(),
  name: z.string(),
  type: z.number(),
  value: z.union([z.number(), z.string(), z.boolean()]),
  defaultValue: z.optional(z.number()),
  min: z.optional(z.number()),
  max: z.optional(z.number()),
  visible: z.optional(z.boolean()),
  editable: z.optional(z.boolean()),
});

export const ParameterHelpSchema = z.object({
  name: z.string(),
  helpText: z.string(),
});

export const ApiResponseSchema = z.object({
  success: z.boolean(),
  message: z.optional(z.string()),
});

export const CommandResponseSchema = z.union([
  ApiResponseSchema,
  z.object({ accepted: z.literal(true) }),
]);

export const SteamResponseSchema = z.extend(ApiResponseSchema, {
  steamMode: z.boolean(),
});

export const PidResponseSchema = z.extend(ApiResponseSchema, {
  pidEnabled: z.boolean(),
});

export const BackflushResponseSchema = z.extend(ApiResponseSchema, {
  backflushOn: z.boolean(),
});

export const ScaleResponseSchema = z.extend(ApiResponseSchema, {
  scaleTareOn: z.optional(z.boolean()),
  scaleCalibrationOn: z.optional(z.boolean()),
});

export const ConfigUploadResponseSchema = z.extend(ApiResponseSchema, {
  restart: z.optional(z.boolean()),
});

export const OtaStatusSchema = z.object({
  status: z.enum([
    "idle",
    "downloading",
    "uploading",
    "processing",
    "complete",
    "error",
  ]),
  progress: z.number(),
  updateInProgress: z.boolean(),
  message: z.optional(z.string()),
  error: z.optional(z.string()),
});

export type TemperatureData = z.infer<typeof TemperatureDataSchema>;
export type HistoryData = z.infer<typeof HistoryDataSchema>;
export type Parameter = z.infer<typeof ParameterSchema>;
export type ParameterHelp = z.infer<typeof ParameterHelpSchema>;
export type ApiResponse = z.infer<typeof ApiResponseSchema>;
export type SteamResponse = z.infer<typeof SteamResponseSchema>;
export type PidResponse = z.infer<typeof PidResponseSchema>;
export type BackflushResponse = z.infer<typeof BackflushResponseSchema>;
export type ScaleResponse = z.infer<typeof ScaleResponseSchema>;
export type ConfigUploadResponse = z.infer<typeof ConfigUploadResponseSchema>;
export type OtaStatus = z.infer<typeof OtaStatusSchema>;

export const ServerParametersSchema = z.array(
  z.object({
    name: z.string(),
    type: z.union([
      z.literal(0),
      z.literal(1),
      z.literal(2),
      z.literal(3),
      z.literal(4),
      z.literal(5),
      z.literal(6),
    ]),
    value: z.union([z.string(), z.number(), z.boolean()]),
    min: z.nullish(z.number()),
    max: z.nullish(z.number()),
    options: z.optional(
      z.array(z.object({ value: z.number(), label: z.string() })),
    ),
  }),
);
export const MachineStatusSchema = z.object({
  brewing: z.optional(z.boolean()),
  pidEnabled: z.optional(z.boolean()),
  steamMode: z.optional(z.boolean()),
  backflushOn: z.optional(z.boolean()),
  isStandby: z.optional(z.boolean()),
  shotsSinceBackflush: z.optional(z.number()),
  backflushReminderThreshold: z.optional(z.number()),
  backflushReminderDue: z.optional(z.boolean()),
});
