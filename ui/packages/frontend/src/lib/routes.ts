/**
 * Shared API routes - mirrors backend routes in WebServerManager.cpp and ota.cpp
 * Single source of truth for route definitions - validates parity with C++ side
 */

export const API_ROUTES = {
  // Machine Control
  STEAM: "/api/steam",
  PID: "/api/pid",
  BACKFLUSH: "/api/backflush",
  SETPOINT: "/api/setpoint",

  // Configuration
  CONFIG: "/api/config",
  CONFIG_DOWNLOAD: "/api/config/download",
  PARAMETERS: "/api/parameters",
  PARAMETER_HELP: "/api/parameter-help",

  // Status & Telemetry
  STATUS: "/api/status",
  HEALTH: "/api/health",
  TEMPERATURES: "/api/temperatures",
  HISTORY: "/api/history",

  // Scale
  SCALE_TARE: "/api/scale/tare",
  SCALE_CALIBRATION: "/api/scale/calibration",

  // OTA
  OTA_STATUS: "/api/ota/status",
  OTA_FIRMWARE: "/api/ota/firmware",
  OTA_FILESYSTEM: "/api/ota/filesystem",
  OTA_URL: "/api/ota/url",

  // System
  RESTART: "/api/restart",
  FACTORY_RESET: "/api/factory-reset",
  WIFI_RESET: "/api/wifi-reset",
  NVS_DEBUG: "/api/nvs-debug",
} as const;

export type ApiRoute = (typeof API_ROUTES)[keyof typeof API_ROUTES];

export function getRoute(route: ApiRoute): string {
  return route;
}