import fs from "node:fs";
import path from "node:path";
import cors from "cors";
import express, {
  type NextFunction,
  type Request,
  type Response,
} from "express";
import multer from "multer";
import parameters from "./parameters";

const app = express();
const PORT = 3001;

// OTA Simulation Configuration
const OTA_CONFIG = {
  // Simulation timing (in milliseconds)
  UPDATE_INTERVAL: 1000, // How often to update progress
  PROCESSING_DELAY: 2000, // Time spent in processing phase
  RESTART_DELAY: 3000, // Time to simulate device restart
  ERROR_RESET_DELAY: 10000, // Time to reset error state

  // Simulation behavior
  FAILURE_RATE: 0.05, // 5% chance of simulated failure
  MIN_PROGRESS_STEP: 5, // Minimum progress increment per step
  MAX_PROGRESS_STEP: 20, // Maximum progress increment per step

  // File size limits (bytes)
  MAX_FIRMWARE_SIZE: 10 * 1024 * 1024, // 10MB
  MAX_FILESYSTEM_SIZE: 5 * 1024 * 1024, // 5MB
};

// Middleware
app.use(cors());
app.use(express.json());
app.use(express.urlencoded({ extended: true }));

// Configure multer for file uploads
const upload = multer({ dest: "uploads/" });

// Type definitions
type ParameterValue = string | number | boolean;

interface Parameter {
  name: string;
  value: ParameterValue;
  section?: number;
  show?: boolean;
  type?: number;
  min?: number;
  max?: number;
  displayName?: string;
}

interface HistoryData {
  currentTemps: number[];
  targetTemps: number[];
  heaterPowers: number[];
}

interface MockState {
  steamMode: boolean;
  pidEnabled: boolean;
  backflushOn: boolean;
  scaleTareOn: boolean;
  scaleCalibrationOn: boolean;
  currentTemp: number;
  targetTemp: number;
  heaterPower: number;
  shotsSinceBackflush: number;
  isStandby: boolean;
  parameters: Parameter[];
  historyData: HistoryData;
}

interface ConfigFile {
  version: string;
  parameters: Record<string, ParameterValue>;
  system: {
    hostname: string;
    auth: {
      enabled: boolean;
      username: string;
      password: string;
    };
  };
  hardware: {
    sensors: {
      scale: {
        enabled: boolean;
      };
    };
  };
}

// Request type extensions
interface ParameterHelpQuery {
  param?: string;
}

interface ParametersQuery {
  filter?: "hardware" | "behavior" | "other";
}

// Mock data store
const mockState: MockState = {
  steamMode: false,
  pidEnabled: true,
  backflushOn: false,
  scaleTareOn: false,
  scaleCalibrationOn: false,
  currentTemp: 93.5,
  targetTemp: 94.0,
  heaterPower: 75.2,
  shotsSinceBackflush: 12,
  isStandby: false,
  parameters: parameters as Parameter[],
  historyData: {
    currentTemps: [],
    targetTemps: [],
    heaterPowers: [],
  },
};

// Generate some mock history data
const generateHistoryData = (): HistoryData => {
  const length = 100;
  const currentTemps: number[] = [];
  const targetTemps: number[] = [];
  const heaterPowers: number[] = [];

  for (let i = 0; i < length; i++) {
    const baseTemp = 94.0;
    const variation = Math.sin(i * 0.1) * 2 + Math.random() * 1;
    currentTemps.push(Math.round((baseTemp + variation) * 100) / 100);
    targetTemps.push(baseTemp);
    heaterPowers.push(Math.round((50 + Math.random() * 50) * 100) / 100);
  }

  return { currentTemps, targetTemps, heaterPowers };
};

mockState.historyData = generateHistoryData();

function isMaintenanceReminderEnabled(
  enabledParam: Parameter | undefined,
): boolean {
  if (enabledParam === undefined) {
    return true;
  }
  const value = enabledParam.value;
  return value === true || value === 1;
}

// Helper function to simulate authentication
const simulateAuth = (
  _req: Request,
  _res: Response,
  next: NextFunction,
): void => {
  // For demo purposes, we'll always pass authentication
  // In a real scenario, you might want to add some basic auth simulation
  next();
};

// Machine control endpoints
app.post("/api/steam", simulateAuth, (_req: Request, res: Response): void => {
  mockState.steamMode = !mockState.steamMode;

  // Update parameter state
  const steamParam = mockState.parameters.find((p) => p.name === "STEAM_MODE");
  if (steamParam) {
    steamParam.value = mockState.steamMode ? 1 : 0;
  }

  console.log(`Steam mode toggled: ${mockState.steamMode ? "ON" : "OFF"}`);
  res.json({ success: true, steamMode: mockState.steamMode });
});

app.post("/api/pid", simulateAuth, (_req: Request, res: Response): void => {
  mockState.pidEnabled = !mockState.pidEnabled;

  // Update parameter state
  const pidParam = mockState.parameters.find((p) => p.name === "pid.enabled");
  if (pidParam) {
    pidParam.value = mockState.pidEnabled ? 1 : 0;
  }

  console.log(`PID toggled: ${mockState.pidEnabled ? "ON" : "OFF"}`);
  res.json({ success: true, pidEnabled: mockState.pidEnabled });
});

app.post(
  "/api/backflush",
  simulateAuth,
  (_req: Request, res: Response): void => {
    const cyclesParam = mockState.parameters.find(
      (p) => p.name === "backflush.cycles",
    );
    const configuredCycles =
      typeof cyclesParam?.value === "number" ? cyclesParam.value : 5;
    const newState = !mockState.backflushOn;

    if (newState && configuredCycles <= 0) {
      res.status(400).json({
        error: "backflush.cycles must be greater than 0",
      });
      return;
    }

    mockState.backflushOn = newState;

    const backflushParam = mockState.parameters.find(
      (p) => p.name === "BACKFLUSH_ON",
    );
    if (backflushParam) {
      backflushParam.value = mockState.backflushOn ? 1 : 0;
    }

    console.log(
      `Backflush mode toggled: ${mockState.backflushOn ? "ON" : "OFF"}`,
    );
    res.json({ success: true, backflushOn: mockState.backflushOn });
  },
);

app.post("/api/wake", simulateAuth, (_req: Request, res: Response): void => {
  mockState.isStandby = false;
  console.log("Machine woken from standby");
  res.json({ success: true });
});

app.post("/api/sleep", simulateAuth, (_req: Request, res: Response): void => {
  mockState.isStandby = true;
  console.log("Machine entered standby");
  res.json({ success: true });
});

// Dev-only helpers for UI testing (mock server)
app.post(
  "/api/dev/complete-backflush-session",
  simulateAuth,
  (_req: Request, res: Response): void => {
    mockState.shotsSinceBackflush = 0;
    console.log("Dev: backflush session complete (shots counter reset)");
    res.json({ success: true, shotsSinceBackflush: 0 });
  },
);

app.post(
  "/api/dev/shots-since-backflush",
  simulateAuth,
  (req: Request, res: Response): void => {
    const count = Number(req.body?.count);
    if (!Number.isFinite(count) || count < 0) {
      res.status(400).json({ success: false, message: "Invalid count" });
      return;
    }
    mockState.shotsSinceBackflush = count;
    console.log(`Dev: shots since backflush set to ${count}`);
    res.json({ success: true, shotsSinceBackflush: count });
  },
);

app.get("/api/status", simulateAuth, (_req: Request, res: Response): void => {
  const thresholdParam = mockState.parameters.find(
    (p) => p.name === "maintenance.backflush_reminder.threshold",
  );
  const enabledParam = mockState.parameters.find(
    (p) => p.name === "maintenance.backflush_reminder.enabled",
  );
  const threshold =
    typeof thresholdParam?.value === "number" ? thresholdParam.value : 50;
  const enabled = isMaintenanceReminderEnabled(enabledParam);
  const due = enabled && mockState.shotsSinceBackflush >= threshold;

  res.json({
    temperature: mockState.currentTemp,
    setpoint: mockState.targetTemp,
    heaterPower: mockState.heaterPower,
    pidEnabled: mockState.pidEnabled,
    steamMode: mockState.steamMode,
    isStandby: mockState.isStandby,
    standbyTime: 0,
    uptime: Date.now(),
    shotsSinceBackflush: mockState.shotsSinceBackflush,
    backflushReminderThreshold: threshold,
    backflushReminderDue: due,
  });
});

app.post(
  "/api/maintenance/reset-backflush-counter",
  simulateAuth,
  (_req: Request, res: Response): void => {
    mockState.shotsSinceBackflush = 0;
    const thresholdParam = mockState.parameters.find(
      (p) => p.name === "maintenance.backflush_reminder.threshold",
    );
    const enabledParam = mockState.parameters.find(
      (p) => p.name === "maintenance.backflush_reminder.enabled",
    );
    const threshold =
      typeof thresholdParam?.value === "number" ? thresholdParam.value : 50;
    const enabled = isMaintenanceReminderEnabled(enabledParam);

    res.json({
      success: true,
      shotsSinceBackflush: 0,
      backflushReminderDue: enabled && 0 >= threshold,
    });
  },
);

// Scale endpoints
app.post(
  "/api/scale/tare",
  simulateAuth,
  (_req: Request, res: Response): void => {
    mockState.scaleTareOn = !mockState.scaleTareOn;
    console.log(`Scale tare: ${mockState.scaleTareOn ? "ON" : "OFF"}`);
    res.json({ success: true, scaleTareOn: mockState.scaleTareOn });
  },
);

app.post(
  "/api/scale/calibration",
  simulateAuth,
  (_req: Request, res: Response): void => {
    mockState.scaleCalibrationOn = !mockState.scaleCalibrationOn;
    console.log(
      `Scale calibration: ${mockState.scaleCalibrationOn ? "ON" : "OFF"}`,
    );
    res.json({
      success: true,
      scaleCalibrationOn: mockState.scaleCalibrationOn,
    });
  },
);

// Data endpoints
app.get(
  "/api/parameters",
  simulateAuth,
  (
    req: Request<
      Record<string, never>,
      Parameter[],
      Record<string, never>,
      ParametersQuery
    >,
    res: Response<Parameter[]>,
  ): void => {
    const filter = req.query.filter;
    let filteredParams = mockState.parameters;

    if (filter) {
      filteredParams = mockState.parameters.filter((param) => {
        if (filter === "hardware") {
          return (
            param.section !== undefined &&
            param.section >= 11 &&
            param.section <= 15
          );
        } else if (filter === "behavior") {
          return (
            param.section !== undefined &&
            param.section >= 0 &&
            param.section <= 9
          );
        } else if (filter === "other") {
          return param.section === 10;
        }
        return true;
      });
    }

    console.log(
      `Parameters requested (filter: ${filter || "none"}), returning ${
        filteredParams.length
      } parameters`,
    );
    res.json(filteredParams);
  },
);

app.post(
  "/api/parameters",
  simulateAuth,
  (req: Request, res: Response): void => {
    console.log("Parameters update:", req.body);

    // Simulate parameter updates
    let hasErrors = false;
    const updates: Array<{ name: string; value: ParameterValue }> = [];

    // Parse form data
    Object.keys(req.body).forEach((key) => {
      const param = mockState.parameters.find((p) => p.name === key);
      if (param && (param.show == null || param.show)) {
        const value = req.body[key];
        try {
          switch (param.type) {
            case 6:
              // kBoolean
              param.value = value === "true" || value === "1";
              break;
            case 4:
              // kCString
              param.value = value;
              break;
            default: {
              // kNumber
              const numValue = parseFloat(value);
              if (
                !Number.isNaN(numValue) &&
                param.min !== undefined &&
                param.max !== undefined &&
                numValue >= param.min &&
                numValue <= param.max
              ) {
                param.value = numValue;
              } else {
                hasErrors = true;
              }
            }
          }
          updates.push({ name: key, value: param.value });
        } catch (error) {
          hasErrors = true;
          console.error(`Error updating parameter ${key}:`, error);
        }
      }
    });

    console.log(`Updated ${updates.length} parameters, errors: ${hasErrors}`);
    res.json({
      success: !hasErrors,
      message: hasErrors ? "Partial Success" : "OK",
    });
  },
);

app.get(
  "/api/parameter-help",
  simulateAuth,
  (
    req: Request<
      Record<string, never>,
      { name: string; helpText: string } | { error: string },
      Record<string, never>,
      ParameterHelpQuery
    >,
    res: Response,
  ): void => {
    const paramName = req.query.param;

    if (!paramName) {
      res.status(422).json({ error: "parameter is missing" });
      return;
    }

    const param = mockState.parameters.find((p) => p.name === paramName);

    if (!param) {
      res.status(404).json({ error: "parameter not found" });
      return;
    }

    const helpText = `This is help text for ${
      param.displayName || paramName
    }. This parameter controls ${
      paramName.includes("pid")
        ? "PID controller behavior"
        : paramName.includes("temp")
          ? "temperature settings"
          : "system behavior"
    }.`;

    console.log(`Help requested for parameter: ${paramName}`);
    res.json({ name: paramName, helpText });
  },
);

app.get(
  "/api/temperatures",
  simulateAuth,
  (_req: Request, res: Response): void => {
    // Alias for /api/temperature for backward compatibility
    const variation = (Math.random() - 0.5) * 0.5;
    mockState.currentTemp = Math.round((93.5 + variation) * 100) / 100;
    mockState.heaterPower = Math.round((75 + Math.random() * 25) * 100) / 100;

    const tempData = {
      currentTemp: mockState.currentTemp,
      targetTemp: mockState.targetTemp,
      heaterPower: mockState.heaterPower,
    };

    res.json(tempData);
  },
);

app.get(
  "/api/history",
  simulateAuth,
  (_req: Request, res: Response<HistoryData>): void => {
    console.log("History data requested");
    res.json(mockState.historyData);
  },
);

// System endpoints
app.post(
  "/api/wifi-reset",
  simulateAuth,
  (_req: Request, res: Response): void => {
    console.log("WiFi reset requested");
    res.json({
      success: true,
      message: "WiFi settings are being reset. Rebooting...",
    });
  },
);

app.get(
  "/api/config/download",
  simulateAuth,
  (_req: Request, res: Response<ConfigFile>): void => {
    const config: ConfigFile = {
      version: "1.0.0",
      parameters: mockState.parameters.reduce(
        (acc: Record<string, ParameterValue>, param) => {
          acc[param.name] = param.value;
          return acc;
        },
        {},
      ),
      system: {
        hostname: "clevercoffee",
        auth: {
          enabled: false,
          username: "admin",
          password: "admin",
        },
      },
      hardware: {
        sensors: {
          scale: {
            enabled: true,
          },
        },
      },
    };

    console.log("Config download requested");
    res.setHeader("Content-Disposition", 'attachment; filename="config.json"');
    res.json(config);
  },
);

app.post(
  "/api/config/upload",
  upload.single("config"),
  simulateAuth,
  (req: Request, res: Response): void => {
    if (!req.file) {
      res.status(400).json({
        success: false,
        message: "No config file uploaded",
      });
      return;
    }

    try {
      const configData = fs.readFileSync(req.file.path, "utf8");
      const config = JSON.parse(configData) as ConfigFile;

      // Simulate config validation
      if (config.parameters) {
        console.log("Config uploaded and validated");
        res.json({
          success: true,
          message: "Configuration validated and applied successfully.",
          restart: true,
        });
      } else {
        res.status(400).json({
          success: false,
          message: "Configuration validation failed. Invalid format.",
          restart: true,
        });
      }

      // Clean up uploaded file
      fs.unlinkSync(req.file.path);
    } catch (error) {
      console.error("Config upload error:", error);
      res.status(400).json({
        success: false,
        message:
          "Configuration validation failed. Please check that all parameter values are within valid ranges.",
        restart: true,
      });

      // Clean up uploaded file
      if (req.file) {
        fs.unlinkSync(req.file.path);
      }
    }
  },
);

app.post("/api/restart", simulateAuth, (_req: Request, res: Response): void => {
  console.log("Restart requested");
  res.json({ success: true, message: "Restarting..." });
});

app.post(
  "/api/factory-reset",
  simulateAuth,
  (_req: Request, res: Response): void => {
    console.log("Factory reset requested");

    // Reset mock state to defaults
    mockState.steamMode = false;
    mockState.pidEnabled = true;
    mockState.backflushOn = false;
    mockState.scaleTareOn = false;
    mockState.scaleCalibrationOn = false;

    // Reset parameters to default values
    mockState.parameters.forEach((param) => {
      if (param.name === "pid.enabled") param.value = 1;
      else if (param.name === "STEAM_MODE") param.value = 0;
      else if (param.name === "BACKFLUSH_ON") param.value = 0;
      // Add more defaults as needed
    });

    res.json({ success: true, message: "Factory reset. Restarting..." });
  },
);

// OTA Update State
interface OTAState {
  status:
    | "idle"
    | "uploading"
    | "downloading"
    | "processing"
    | "complete"
    | "error";
  type: "firmware" | "filesystem" | "url" | "";
  progress: number;
  uploadedSize: number;
  totalSize: number;
  error?: string;
  updateInProgress: boolean;
  filesystemPartition: string;
}

const otaState: OTAState = {
  status: "idle",
  type: "",
  progress: 0,
  uploadedSize: 0,
  totalSize: 0,
  updateInProgress: false,
  filesystemPartition: "spiffs",
};

// Helper function to simulate OTA progress
const simulateOTAProgress = (
  type: "firmware" | "filesystem" | "url",
  totalSize: number = 0,
): Promise<void> => {
  return new Promise((resolve, reject) => {
    otaState.status = type === "url" ? "downloading" : "uploading";
    otaState.type = type;
    otaState.progress = 0;
    otaState.uploadedSize = 0;
    otaState.totalSize = totalSize;
    otaState.updateInProgress = true;
    otaState.error = undefined;

    console.log(
      `🔄 Starting ${type} OTA update simulation (${totalSize} bytes)`,
    );

    let currentProgress = 0;
    const interval = setInterval(() => {
      const progressStep =
        Math.random() *
          (OTA_CONFIG.MAX_PROGRESS_STEP - OTA_CONFIG.MIN_PROGRESS_STEP) +
        OTA_CONFIG.MIN_PROGRESS_STEP;
      currentProgress += progressStep;

      if (currentProgress >= 90) {
        // Switch to processing phase
        otaState.status = "processing";
        otaState.progress = 95;
        otaState.uploadedSize = totalSize;

        setTimeout(() => {
          // Complete the update
          otaState.status = "complete";
          otaState.progress = 100;
          console.log(`✅ ${type} OTA update completed successfully`);

          setTimeout(() => {
            // Reset after 3 seconds (simulating device restart)
            otaState.status = "idle";
            otaState.type = "";
            otaState.progress = 0;
            otaState.uploadedSize = 0;
            otaState.totalSize = 0;
            otaState.updateInProgress = false;
            console.log(`🔄 Device restart simulation complete`);
            resolve();
          }, OTA_CONFIG.RESTART_DELAY);
        }, OTA_CONFIG.PROCESSING_DELAY);

        clearInterval(interval);
      } else {
        otaState.progress = Math.min(90, currentProgress);
        otaState.uploadedSize = Math.floor(
          (otaState.progress / 100) * totalSize,
        );
      }
    }, OTA_CONFIG.UPDATE_INTERVAL);

    // Simulate random failure
    if (Math.random() < OTA_CONFIG.FAILURE_RATE) {
      setTimeout(
        () => {
          clearInterval(interval);
          otaState.status = "error";
          otaState.error = "Simulated update failure";
          otaState.updateInProgress = false;
          console.log(`❌ ${type} OTA update failed (simulated)`);

          setTimeout(() => {
            // Reset error state after configured delay
            otaState.status = "idle";
            otaState.type = "";
            otaState.progress = 0;
            otaState.uploadedSize = 0;
            otaState.totalSize = 0;
            otaState.error = undefined;
          }, OTA_CONFIG.ERROR_RESET_DELAY);

          reject(new Error("Simulated update failure"));
        },
        Math.random() * 5000 + 2000,
      ); // Fail after 2-7 seconds
    }
  });
};

// OTA Endpoints
app.post(
  "/api/ota/firmware",
  upload.single("firmware"),
  simulateAuth,
  (req: Request, res: Response): void => {
    console.log("🔧 Firmware OTA update requested");

    // Check if update is already in progress
    if (otaState.updateInProgress) {
      console.log("❌ OTA update already in progress");
      res.status(409).json({
        success: false,
        message:
          "OTA update already in progress. Please wait for current update to complete.",
      });
      return;
    }

    if (!req.file) {
      res.status(400).json({
        success: false,
        message: "No firmware file provided",
      });
      return;
    }

    // Validate file extension
    if (!req.file.originalname.toLowerCase().endsWith(".bin")) {
      res.status(400).json({
        success: false,
        message: "Invalid firmware file. Expected .bin extension.",
      });
      return;
    }

    // Validate file size
    const fileSize = req.file.size;
    if (fileSize > OTA_CONFIG.MAX_FIRMWARE_SIZE) {
      res.status(400).json({
        success: false,
        message: `Firmware file is too large. Maximum size is ${Math.floor(
          OTA_CONFIG.MAX_FIRMWARE_SIZE / 1024 / 1024,
        )}MB.`,
      });
      return;
    }
    console.log(
      `📁 Firmware file: ${req.file.originalname} (${fileSize} bytes)`,
    );

    // Start the simulation
    simulateOTAProgress("firmware", fileSize)
      .then(() => {
        // This won't be reached due to the async nature, but kept for completeness
      })
      .catch((error) => {
        console.error("Firmware update simulation failed:", error);
      });

    // Clean up uploaded file
    fs.unlinkSync(req.file.path);

    res.json({
      success: true,
      message: "Update successful. Device will restart.",
    });
  },
);

app.post(
  "/api/ota/filesystem",
  upload.single("filesystem"),
  simulateAuth,
  (req: Request, res: Response): void => {
    console.log("🗂️ Filesystem OTA update requested");

    // Check if update is already in progress
    if (otaState.updateInProgress) {
      console.log("❌ OTA update already in progress");
      res.status(409).json({
        success: false,
        message:
          "OTA update already in progress. Please wait for current update to complete.",
      });
      return;
    }

    if (!req.file) {
      res.status(400).json({
        success: false,
        message: "No filesystem file provided",
      });
      return;
    }

    // Validate file extension
    const validExtensions = [".bin", ".img"];
    const isValidFile = validExtensions.some((ext) =>
      req.file?.originalname.toLowerCase().endsWith(ext),
    );

    if (!isValidFile) {
      res.status(400).json({
        success: false,
        message: "Invalid filesystem file. Expected .bin or .img extension.",
      });
      return;
    }

    // Validate file size
    const fileSize = req.file.size;
    if (fileSize > OTA_CONFIG.MAX_FILESYSTEM_SIZE) {
      res.status(400).json({
        success: false,
        message: `Filesystem file is too large. Maximum size is ${Math.floor(
          OTA_CONFIG.MAX_FILESYSTEM_SIZE / 1024 / 1024,
        )}MB.`,
      });
      return;
    }
    console.log(
      `📁 Filesystem file: ${req.file.originalname} (${fileSize} bytes)`,
    );

    // Start the simulation
    simulateOTAProgress("filesystem", fileSize)
      .then(() => {
        // This won't be reached due to the async nature, but kept for completeness
      })
      .catch((error) => {
        console.error("Filesystem update simulation failed:", error);
      });

    // Clean up uploaded file
    fs.unlinkSync(req.file.path);

    res.json({
      success: true,
      message: "Filesystem update successful. Device will restart.",
    });
  },
);

app.post("/api/ota/url", simulateAuth, (req: Request, res: Response): void => {
  console.log("🌐 URL OTA update requested");

  // Check if update is already in progress
  if (otaState.updateInProgress) {
    console.log("❌ OTA update already in progress");
    res.status(409).json({
      success: false,
      message:
        "OTA update already in progress. Please wait for current update to complete.",
    });
    return;
  }

  const url = req.body.url;
  const updateType = req.body.type === "filesystem" ? "filesystem" : "firmware";
  if (!url) {
    res.status(400).json({
      success: false,
      message: "URL parameter missing",
    });
    return;
  }

  if (!url.trim()) {
    res.status(400).json({
      success: false,
      message: "Empty URL provided",
    });
    return;
  }

  console.log(`🔗 ${updateType} URL: ${url}`);

  // Simulate a firmware size based on URL
  const simulatedSize =
    updateType === "filesystem"
      ? Math.floor(Math.random() * 500000) + 300000 // 300KB - 800KB
      : Math.floor(Math.random() * 1000000) + 500000; // 500KB - 1.5MB

  // Start the simulation
  otaState.type = updateType;
  simulateOTAProgress("url", simulatedSize)
    .then(() => {
      // This won't be reached due to the async nature, but kept for completeness
    })
    .catch((error) => {
      console.error("URL update simulation failed:", error);
    });

  res.json({
    success: true,
    message: "Update successful. Device will restart.",
  });
});

app.get(
  "/api/ota/status",
  simulateAuth,
  (_req: Request, res: Response): void => {
    res.json({
      updating: otaState.updateInProgress,
      updateInProgress: otaState.updateInProgress,
      progress: otaState.progress,
      status: otaState.status,
      type: otaState.type,
      uploadedSize: otaState.uploadedSize,
      totalSize: otaState.totalSize,
      filesystemPartition: otaState.filesystemPartition,
      ...(otaState.error && { error: otaState.error }),
    });
  },
);

// Data endpoints
app.get("/api/health", simulateAuth, (_req: Request, res: Response): void => {
  res.json({});
});

// SSE endpoint for temperature and heater power
app.get("/events", (req: Request, res: Response) => {
  res.setHeader("Content-Type", "text/event-stream");
  res.setHeader("Cache-Control", "no-cache");
  res.setHeader("Connection", "keep-alive");

  // Send initial hello event
  res.write(`event: ping\ndata: hello\n\n`);

  // Send temperature data every 2 seconds
  const interval = setInterval(() => {
    const variation = (Math.random() - 0.5) * 0.5;
    mockState.currentTemp = Math.round((93.5 + variation) * 100) / 100;
    mockState.heaterPower = Math.round((75 + Math.random() * 25) * 100) / 100;
    const tempData = {
      currentTemp: mockState.currentTemp,
      targetTemp: mockState.targetTemp,
      heaterPower: mockState.heaterPower,
    };
    res.write(`event: new_temps\ndata: ${JSON.stringify(tempData)}\n\n`);
  }, 2000);

  req.on("close", () => {
    clearInterval(interval);
    res.end();
  });
});

// 404 handler for API routes
app.use("/api/*path", (req: Request, res: Response): void => {
  console.log(`404 - API endpoint not found: ${req.method} ${req.path}`);
  res.status(404).json({ error: "API endpoint not found" });
});

// Serve static files from UI dist folder if available
const uiDistPath = path.join(process.cwd(), "../ui/dist");
if (fs.existsSync(uiDistPath)) {
  app.use(express.static(uiDistPath));

  // SPA fallback for non-API routes
  app.get("/*path", (_req: Request, res: Response): void => {
    res.sendFile(path.join(uiDistPath, "index.html"));
  });
} else {
  // Simple message if UI dist not available
  app.get("/*path", (_req: Request, res: Response): void => {
    res.send(`
      <h1>CleverCoffee Mock Server</h1>
      <p>Mock API server is running on port ${PORT}</p>
      <p>UI not found. Build the UI first with: <code>cd ../ui && npm run build</code></p>
      <h2>Available API Endpoints:</h2>
      <ul>
        <li>POST /api/steam - Toggle steam mode</li>
        <li>POST /api/pid - Toggle PID control</li>
        <li>POST /api/backflush - Toggle backflush mode</li>
        <li>POST /api/scale/tare - Tare scale</li>
        <li>POST /api/scale/calibration - Start scale calibration</li>
        <li>GET /api/parameters - Get parameters</li>
        <li>POST /api/parameters - Update parameters</li>
        <li>GET /api/parameter-help?param=name - Get parameter help</li>
        <li>GET /api/temperature - Get current temperature</li>
        <li>GET /api/history - Get temperature history</li>
        <li>POST /api/wifi-reset - Reset WiFi settings</li>
        <li>GET /api/config/download - Download configuration</li>
        <li>POST /api/config/upload - Upload configuration</li>
        <li>POST /api/restart - Restart device</li>
        <li>POST /api/factory-reset - Factory reset</li>
        <li>POST /api/ota/firmware - Upload firmware for OTA update</li>
        <li>POST /api/ota/filesystem - Upload filesystem for OTA update</li>
        <li>POST /api/ota/url - Start OTA update from URL</li>
        <li>GET /api/ota/status - Get OTA update status</li>
      </ul>
    `);
  });
}

// Start server
app.listen(PORT, (): void => {
  console.log(
    `🚀 CleverCoffee Mock Server running on http://localhost:${PORT}`,
  );
  console.log(`📡 API endpoints available at http://localhost:${PORT}/api/*`);
  console.log(`🔄 OTA update simulation enabled`);
  console.log(
    `💡 Use this server for frontend development when ESP32 hardware is not available`,
  );

  // Create uploads directory if it doesn't exist
  if (!fs.existsSync("uploads")) {
    fs.mkdirSync("uploads");
  }
});

// Graceful shutdown
process.on("SIGINT", (): void => {
  console.log("\n👋 Shutting down mock server...");
  process.exit(0);
});
