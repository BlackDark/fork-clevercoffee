/**
 * Shared API routes - mirrors `ROUTES` in crates/web/src/lib.rs
 */

export const API_ROUTES = {
  // Machine Control
  STEAM: "/steam",
  PID: "/pid",
  BACKFLUSH: "/backflush",
  SETPOINT: "/setpoint",
  WAKE: "/wake",
  SLEEP: "/sleep",

  // Configuration
  CONFIG: "/config",
  CONFIG_DOWNLOAD: "/config/download",
  PARAMETERS: "/parameters",
  PARAMETER_HELP: "/parameter-help",

  // Status & Telemetry
  STATUS: "/status",
  HEALTH: "/health",
  TEMPERATURES: "/temperatures",
  HISTORY: "/history",

  // Scale
  SCALE_TARE: "/scale/tare",
  SCALE_CALIBRATION: "/scale/calibration",

  // OTA
  OTA_STATUS: "/ota/status",
  OTA_FIRMWARE: "/ota/firmware",

  // System
  RESTART: "/restart",
  FACTORY_RESET: "/factory-reset",
  WIFI_RESET: "/wifi-reset",

  // Maintenance
  MAINTENANCE_RESET_BACKFLUSH: "/maintenance/reset-backflush-counter",
} as const;

export type ApiRoute = (typeof API_ROUTES)[keyof typeof API_ROUTES];

export function getRoute(route: ApiRoute): string {
  return route;
}

export function getApiRoute(route: ApiRoute): string {
  return `/api${route}`;
}
