import express, { Request, Response, NextFunction } from "express";
import cors from "cors";
import multer from "multer";
import fs from "fs";
import path from "path";
import parameters from "./parameters";

const app = express();
const PORT = 3001;

// Middleware
app.use(cors());
app.use(express.json());
app.use(express.urlencoded({ extended: true }));

// Configure multer for file uploads
const upload = multer({ dest: "uploads/" });

// Type definitions
interface Parameter {
  name: string;
  value: any;
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
  parameters: Parameter[];
  historyData: HistoryData;
}

interface ConfigFile {
  version: string;
  parameters: Record<string, any>;
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
let mockState: MockState = {
  steamMode: false,
  pidEnabled: true,
  backflushOn: false,
  scaleTareOn: false,
  scaleCalibrationOn: false,
  currentTemp: 93.5,
  targetTemp: 94.0,
  heaterPower: 75.2,
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

// Helper function to simulate authentication
const simulateAuth = (
  req: Request,
  res: Response,
  next: NextFunction
): void => {
  // For demo purposes, we'll always pass authentication
  // In a real scenario, you might want to add some basic auth simulation
  next();
};

// Machine control endpoints
app.post("/api/steam", simulateAuth, (req: Request, res: Response): void => {
  mockState.steamMode = !mockState.steamMode;

  // Update parameter state
  const steamParam = mockState.parameters.find((p) => p.name === "STEAM_MODE");
  if (steamParam) {
    steamParam.value = mockState.steamMode ? 1 : 0;
  }

  console.log(`Steam mode toggled: ${mockState.steamMode ? "ON" : "OFF"}`);
  res.json({ success: true, steamMode: mockState.steamMode });
});

app.post("/api/pid", simulateAuth, (req: Request, res: Response): void => {
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
  (req: Request, res: Response): void => {
    mockState.backflushOn = !mockState.backflushOn;

    // Update parameter state
    const backflushParam = mockState.parameters.find(
      (p) => p.name === "BACKFLUSH_ON"
    );
    if (backflushParam) {
      backflushParam.value = mockState.backflushOn ? 1 : 0;
    }

    console.log(
      `Backflush mode toggled: ${mockState.backflushOn ? "ON" : "OFF"}`
    );
    res.json({ success: true, backflushOn: mockState.backflushOn });
  }
);

// Scale endpoints
app.post(
  "/api/scale/tare",
  simulateAuth,
  (req: Request, res: Response): void => {
    mockState.scaleTareOn = !mockState.scaleTareOn;
    console.log(`Scale tare: ${mockState.scaleTareOn ? "ON" : "OFF"}`);
    res.json({ success: true, scaleTareOn: mockState.scaleTareOn });
  }
);

app.post(
  "/api/scale/calibration",
  simulateAuth,
  (req: Request, res: Response): void => {
    mockState.scaleCalibrationOn = !mockState.scaleCalibrationOn;
    console.log(
      `Scale calibration: ${mockState.scaleCalibrationOn ? "ON" : "OFF"}`
    );
    res.json({
      success: true,
      scaleCalibrationOn: mockState.scaleCalibrationOn,
    });
  }
);

// Data endpoints
app.get(
  "/api/parameters",
  simulateAuth,
  (
    req: Request<{}, Parameter[], {}, ParametersQuery>,
    res: Response<Parameter[]>
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
      } parameters`
    );
    res.json(filteredParams);
  }
);

app.post(
  "/api/parameters",
  simulateAuth,
  (req: Request, res: Response): void => {
    console.log("Parameters update:", req.body);

    // Simulate parameter updates
    let hasErrors = false;
    const updates: Array<{ name: string; value: any }> = [];

    // Parse form data
    Object.keys(req.body).forEach((key) => {
      const param = mockState.parameters.find((p) => p.name === key);
      if (param && param.show) {
        const value = req.body[key];
        try {
          if (param.type === 4) {
            // kCString
            param.value = value;
          } else {
            const numValue = parseFloat(value);
            if (
              !isNaN(numValue) &&
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
  }
);

app.get(
  "/api/parameter-help",
  simulateAuth,
  (
    req: Request<
      {},
      { name: string; helpText: string } | { error: string },
      {},
      ParameterHelpQuery
    >,
    res: Response
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
  }
);

app.get(
  "/api/temperatures",
  simulateAuth,
  (req: Request, res: Response): void => {
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
  }
);

app.get(
  "/api/history",
  simulateAuth,
  (req: Request, res: Response<HistoryData>): void => {
    console.log("History data requested");
    res.json(mockState.historyData);
  }
);

// System endpoints
app.post(
  "/api/wifi-reset",
  simulateAuth,
  (req: Request, res: Response): void => {
    console.log("WiFi reset requested");
    res.json({
      success: true,
      message: "WiFi settings are being reset. Rebooting...",
    });
  }
);

app.get(
  "/api/config/download",
  simulateAuth,
  (req: Request, res: Response<ConfigFile>): void => {
    const config: ConfigFile = {
      version: "1.0.0",
      parameters: mockState.parameters.reduce(
        (acc: Record<string, any>, param) => {
          acc[param.name] = param.value;
          return acc;
        },
        {}
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
  }
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
  }
);

app.post("/api/restart", simulateAuth, (req: Request, res: Response): void => {
  console.log("Restart requested");
  res.json({ success: true, message: "Restarting..." });
});

app.post(
  "/api/factory-reset",
  simulateAuth,
  (req: Request, res: Response): void => {
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
  }
);

// Data endpoints
app.get("/api/health", simulateAuth, (req: Request, res: Response): void => {
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
  app.get("/*path", (req: Request, res: Response): void => {
    res.sendFile(path.join(uiDistPath, "index.html"));
  });
} else {
  // Simple message if UI dist not available
  app.get("/*path", (req: Request, res: Response): void => {
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
      </ul>
    `);
  });
}

// Start server
app.listen(PORT, (): void => {
  console.log(
    `🚀 CleverCoffee Mock Server running on http://localhost:${PORT}`
  );
  console.log(`📡 API endpoints available at http://localhost:${PORT}/api/*`);
  console.log(
    `💡 Use this server for frontend development when ESP32 hardware is not available`
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
