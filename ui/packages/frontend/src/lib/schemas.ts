import { z } from "zod";

export const TemperatureDataSchema = z.object({
  currentTemp: z.number(),
  targetTemp: z.number(),
  heaterPower: z.number(),
});

export const HistoryDataSchema = z.object({
  data: z.array(z.unknown()).optional(),
  currentTemps: z.array(z.number()),
  targetTemps: z.array(z.number()),
  heaterPowers: z.number().array(),
});

export const ParameterSchema = z.object({
  id: z.string(),
  name: z.string(),
  type: z.string(),
  value: z.number(),
  defaultValue: z.number().optional(),
  min: z.number().optional(),
  max: z.number().optional(),
  visible: z.boolean().optional(),
  editable: z.boolean().optional(),
});

export const ParameterHelpSchema = z.object({
  name: z.string(),
  helpText: z.string(),
});

export const ApiResponseSchema = z.object({
  success: z.boolean(),
  message: z.string().optional(),
});

export const SteamResponseSchema = ApiResponseSchema.extend({
  steamMode: z.boolean(),
});

export const PidResponseSchema = ApiResponseSchema.extend({
  pidEnabled: z.boolean(),
});

export const BackflushResponseSchema = ApiResponseSchema.extend({
  backflushOn: z.boolean(),
});

export const ScaleResponseSchema = ApiResponseSchema.extend({
  scaleTareOn: z.boolean().optional(),
  scaleCalibrationOn: z.boolean().optional(),
});

export const ConfigUploadResponseSchema = ApiResponseSchema.extend({
  restart: z.boolean().optional(),
});

export const OtaStatusSchema = z.object({
  status: z.enum(["idle", "downloading", "uploading", "processing", "complete", "error"]),
  progress: z.number(),
  updateInProgress: z.boolean(),
  message: z.string().optional(),
  error: z.string().optional(),
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