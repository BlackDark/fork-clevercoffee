/**
 * Type definitions for CleverCoffee API responses
 */

export interface TemperatureData {
  currentTemp: number;
  targetTemp: number;
  heaterPower: number;
}

export interface HistoryData {
  currentTemps: number[];
  targetTemps: number[];
  heaterPowers: number[];
}

export interface ParameterHelpResponse {
  name: string;
  helpText: string;
}

export interface ApiResponse {
  success: boolean;
  message?: string;
}

export interface SteamResponse extends ApiResponse {
  steamMode: boolean;
}

export interface PidResponse extends ApiResponse {
  pidEnabled: boolean;
}

export interface BackflushResponse extends ApiResponse {
  backflushOn: boolean;
}

export interface ScaleResponse extends ApiResponse {
  scaleTareOn?: boolean;
  scaleCalibrationOn?: boolean;
}

export interface ConfigUploadResponse extends ApiResponse {
  restart?: boolean;
}
