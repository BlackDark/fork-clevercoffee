/**
 * @file WebServerManager.cpp
 * @brief Implementation of the embedded web server manager
 */

#include "clevercoffee/network/WebServerManager.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/network/CleverCoffeeWiFiManager.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/utils/SystemUtils.h"
#include "clevercoffee/utils/helperUtils.h"

#include <Arduino.h>
#include <ArduinoJson.h>
#include <FS.h>
#include <Preferences.h>
#include <unordered_map>

#define JSON_BUFFER_SIZE 512
#define PATH_BUFFER_SIZE 128

namespace {

void requestNormalOperation(CleverCoffee::SystemContext* context) {
    if (context && context->machineStateContext()) {
        context->machineStateContext()->setNormalOperationRequested(true);
    }
}

void requestStandby(CleverCoffee::SystemContext* context) {
    if (context && context->machineStateContext()) {
        context->machineStateContext()->setStandbyRequested(true);
    }
}

bool isMachineStateReady(CleverCoffee::SystemContext* context) {
    return context && context->machineStateContext();
}

} // namespace

// Memory monitoring function
void logMemoryUsage(const char* location) {
    size_t freeHeap    = ESP.getFreeHeap();
    size_t minFreeHeap = ESP.getMinFreeHeap();

    LOGF(INFO, "Memory at %s - Free: %d, Min Free: %d", location, freeHeap, minFreeHeap);

    if (freeHeap < 10000) { // Less than 10KB free
        LOGF(WARNING, "Low memory condition detected!");
    }
}

// ==================== CONTENT TYPE DETECTION ====================

#if !FRONTEND_PREPROCESSING
const char* getContentType(const String& path) {
    if (path.endsWith(".html")) return "text/html";
    if (path.endsWith(".css")) return "text/css";
    if (path.endsWith(".js")) return "application/javascript";
    if (path.endsWith(".json")) return "application/json";
    if (path.endsWith(".png")) return "image/png";
    if (path.endsWith(".jpg") || path.endsWith(".jpeg")) return "image/jpeg";
    if (path.endsWith(".gif")) return "image/gif";
    if (path.endsWith(".svg")) return "image/svg+xml";
    if (path.endsWith(".ico")) return "image/x-icon";
    if (path.endsWith(".woff")) return "font/woff";
    if (path.endsWith(".woff2")) return "font/woff2";
    if (path.endsWith(".ttf")) return "font/ttf";
    if (path.endsWith(".eot")) return "application/vnd.ms-fontobject";
    if (path.endsWith(".webp")) return "image/webp";
    if (path.endsWith(".avif")) return "image/avif";
    if (path.endsWith(".webm")) return "video/webm";

    return "text/plain";
}
#endif

// ==================== API RESPONSE BUILDERS ====================

#include "clevercoffee/utils/ApiResponses.h"

// ==================== TEMPERATURE HISTORY ====================

class TemperatureHistory {
  private:
    static constexpr size_t HISTORY_SIZE  = 600;
    static constexpr size_t SKIP_INTERVAL = 2;

    struct HistoryPoint {
        float currentTemp;
        float targetTemp;
        float heaterPower;
    };

    HistoryPoint history[HISTORY_SIZE];
    size_t       currentIndex = 0;
    size_t       valueCount   = 0;
    size_t       skipCounter  = 0;

  public:
    void addPoint(double currentTemp, double targetTemp, double heaterPower) {
        if (++skipCounter <= SKIP_INTERVAL) return;

        skipCounter           = 0;
        history[currentIndex] = {
            static_cast<float>(currentTemp), static_cast<float>(targetTemp), static_cast<float>(heaterPower)};

        currentIndex = (currentIndex + 1) % HISTORY_SIZE;
        if (valueCount < HISTORY_SIZE) valueCount++;
    }

    void generateJson(JsonDocument& doc) const {
        auto currentTemps = doc["currentTemps"].to<JsonArray>();
        auto targetTemps  = doc["targetTemps"].to<JsonArray>();
        auto heaterPowers = doc["heaterPowers"].to<JsonArray>();

        size_t startIdx =
            (currentIndex >= valueCount) ? (currentIndex - valueCount) : (HISTORY_SIZE - (valueCount - currentIndex));

        for (size_t i = 0; i < valueCount; i++) {
            size_t      idx   = (startIdx + i) % HISTORY_SIZE;
            const auto& point = history[idx];

            currentTemps.add(round2(point.currentTemp));
            targetTemps.add(round2(point.targetTemp));
            heaterPowers.add(round2(point.heaterPower));
        }
    }
};

static TemperatureHistory tempHistory;

// ==================== SAFE JSON OPERATIONS ====================

bool safeSerializeJson(const JsonDocument& doc, String& output) {
    try {
        if (doc.overflowed()) {
            LOGF(ERROR, "JSON document overflowed before serialization");
            return false;
        }

        size_t requiredSize = measureJson(doc) + 16;
        if (requiredSize > ESP.getFreeHeap() / 2) {
            LOGF(ERROR, "Insufficient memory for JSON serialization");
            return false;
        }

        output.reserve(requiredSize);
        serializeJson(doc, output);
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "JSON serialization failed: %s", e.what());
        return false;
    }
}

WebServerManager::WebServerManager(uint16_t port)
    : server_(nullptr), events_(nullptr), corsMiddleware_(nullptr), authMiddleware_(nullptr), port_(port),
      isRunning_(false), littleFSAvailable_(false) {}

WebServerManager::~WebServerManager() {
    stop();
    // Smart pointers automatically clean up
}

bool WebServerManager::initialize(bool littleFSReady) {
    logMemoryUsage("WebServerManager initialize start");

    if (isRunning_) {
        LOG(WARNING, "WebServerManager already running");
        return true;
    }

    LOG(INFO, "Initializing WebServerManager");

    // Check LittleFS availability
    littleFSAvailable_ = littleFSReady;
    if (!littleFSAvailable_) {
        if (LittleFS.begin()) {
            littleFSAvailable_ = true;
            LOG(INFO, "LittleFS initialized by WebServerManager");
        } else {
            LOG(WARNING, "LittleFS not available - static files will not be served");
        }
    }

    try {
        // Create server instance
        server_ = std::make_unique<AsyncWebServer>(port_);
        if (!server_) {
            LOG(ERROR, "Failed to create AsyncWebServer instance");
            return false;
        }

        // Setup middleware
        setupMiddleware();

        // Setup event source for real-time updates
        setupEventSource();

        // Setup API routes
        setupApiRoutes();

        // Setup static file serving (if LittleFS is available)
        if (littleFSAvailable_) {
            setupStaticRoutes();
        }

        // Handle 404 errors
        server_->onNotFound([this](AsyncWebServerRequest* request) { handleNotFound(request); });

        // Start server
        server_->begin();
        isRunning_ = true;

        LOGF(INFO, "WebServerManager started on port %d", port_);
        return true;

    } catch (const std::exception& e) {
        LOGF(ERROR, "WebServerManager initialization failed: %s", e.what());
        stop();
        return false;
    }

    logMemoryUsage("WebServerManager initialize done");
}

void WebServerManager::stop() {
    if (!isRunning_) {
        return;
    }

    LOG(INFO, "Stopping WebServerManager");

    if (server_) {
        server_->end();
        server_.reset();
    }

    events_.reset();
    isRunning_ = false;

    LOG(INFO, "WebServerManager stopped");
}

void WebServerManager::setupMiddleware() {
    LOG(DEBUG, "Setting up middleware");

    // CORS middleware - allow all origins for development
    corsMiddleware_ = std::make_unique<AsyncCorsMiddleware>();
    corsMiddleware_->setOrigin("*");
    corsMiddleware_->setMethods("GET,POST,PUT,DELETE,OPTIONS");
    corsMiddleware_->setHeaders("Content-Type,Authorization,X-Requested-With");
    server_->addMiddleware(corsMiddleware_.get());

    // Authentication middleware (if enabled)
    if (Config::getInstance().systemAuthEnabled.get()) {
        String username = Config::getInstance().systemAuthUsername.get();
        String password = Config::getInstance().systemAuthPassword.get();

        if (!username.isEmpty() && !password.isEmpty()) {
            authMiddleware_ = std::make_unique<AsyncAuthenticationMiddleware>();
            authMiddleware_->setUsername(username.c_str());
            authMiddleware_->setPassword(password.c_str());
            authMiddleware_->setRealm("CleverCoffee");
            server_->addMiddleware(authMiddleware_.get());
            LOG(INFO, "Web authentication enabled");
        } else {
            LOG(WARNING, "Web authentication enabled but credentials not set");
        }
    }
}

void WebServerManager::setupEventSource() {
    LOG(DEBUG, "Setting up event source");

    events_ = std::make_unique<AsyncEventSource>("/events");
    if (!events_) {
        LOG(ERROR, "Failed to create event source");
        return;
    }

    events_->onConnect([](AsyncEventSourceClient* client) {
        if (client->lastId()) {
            LOGF(DEBUG, "Client reconnected with last message ID: %u", client->lastId());
        } else {
            LOG(DEBUG, "New client connected to event source");
        }

        // Send current state to new client
        client->send("hello", "Connected to CleverCoffee", millis());
    });

    server_->addHandler(events_.get());
    LOG(INFO, "Event source setup complete");
}

void WebServerManager::setupApiRoutes() {
    LOG(DEBUG, "Setting up API routes");

    // System status endpoint
    server_->on("/api/status", HTTP_GET, [this](AsyncWebServerRequest* request) {
        JsonDocument doc;

        if (!systemContext_) {
            request->send(500, "application/json", "{\"error\":\"System context not available\"}");
            return;
        }

        // WebServer starts before StateMachine registers machineStateContext in setup().
        MachineStateId currentState = MachineStateId::INIT;
        if (systemContext_->machineStateContext()) {
            currentState = systemContext_->machineStateContext()->getCurrentStateId();
        }

        double temperature = systemContext_->processTemperature();
        double setpoint    = systemContext_->processSetpoint();
        double heaterPower = systemContext_->processPidOutput() / 10.0;
        bool   pidEnabled  = Config::getInstance().pidEnabled.get();
        if (systemContext_->processController()) {
            temperature = systemContext_->processController()->getCurrentTemperature();
            setpoint    = systemContext_->processController()->getSetpoint();
            heaterPower = systemContext_->processController()->getPIDOutput() / 10.0;
            pidEnabled  = systemContext_->processController()->isPIDEnabled();
        }

        doc["temperature"]                = temperature;
        doc["setpoint"]                   = setpoint;
        doc["heaterPower"]                = heaterPower;
        doc["machineState"]               = static_cast<int>(currentState);
        doc["isStandby"]                  = (currentState == MachineStateId::STANDBY);
        doc["standbyTime"]                = systemContext_->standbyCoordinator().getRemainingTimeMillis();
        doc["pidEnabled"]                 = pidEnabled;
        doc["steamMode"]                  = systemContext_->steamMode();
        doc["uptime"]                     = millis();
        doc["shotsSinceBackflush"]        = systemContext_->maintenanceCoordinator().getShotsSinceBackflush();
        doc["backflushReminderThreshold"] = Config::getInstance().maintenanceBackflushReminderThreshold.get();
        doc["backflushReminderDue"]       = systemContext_->maintenanceCoordinator().isReminderDue();

        if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
            doc["weight"]     = systemContext_->sensorCoordinator().getWeight();
            doc["brewWeight"] = systemContext_->sensorCoordinator().getBrewWeight();
        }

        String response;
        serializeJson(doc, response);
        request->send(200, "application/json", response);
    });

    // Configuration endpoints
    server_->on("/api/config", HTTP_GET, [](AsyncWebServerRequest* request) {
        String configJson = Config::getInstance().exportToJson();
        request->send(200, "application/json", configJson);
    });

    server_->on("/api/config", HTTP_POST, [this](AsyncWebServerRequest* request) {
        if (request->hasParam("body", true)) {
            String body = request->getParam("body", true)->value();
            if (Config::getInstance().importFromJson(body)) {
                if (systemContext_) {
                    systemContext_->standbyCoordinator().reset();
                }
                request->send(200, "application/json", "{\"success\":true}");
            } else {
                request->send(400, "application/json", "{\"error\":\"Invalid configuration\"}");
            }
        } else {
            request->send(400, "application/json", "{\"error\":\"No body provided\"}");
        }
    });

    // Setpoint adjustment
    server_->on("/api/setpoint", HTTP_POST, [this](AsyncWebServerRequest* request) {
        if (request->hasParam("value", true)) {
            double newSetpoint = request->getParam("value", true)->value().toDouble();
            if (newSetpoint >= 0 && newSetpoint <= 150) {
                if (systemContext_) {
                    systemContext_->setProcessSetpoint(newSetpoint);
                    systemContext_->standbyCoordinator().reset();
                    requestNormalOperation(systemContext_);
                }
                (void)Config::getInstance().brewSetpoint.set(newSetpoint);
                request->send(200, "application/json", "{\"success\":true}");
            } else {
                request->send(400, "application/json", "{\"error\":\"Invalid setpoint value\"}");
            }
        } else {
            request->send(400, "application/json", "{\"error\":\"No value provided\"}");
        }
    });

    // Health check endpoint
    server_->on("/api/health", HTTP_GET, [](AsyncWebServerRequest* request) { request->send(200); });

    // Wake endpoint - reset standby timer to wake machine from standby
    server_->on("/api/wake", HTTP_POST, [this](AsyncWebServerRequest* request) {
        if (systemContext_) {
            systemContext_->standbyCoordinator().reset();
            requestNormalOperation(systemContext_);
            LOG(INFO, "Wake from standby via API");
            request->send(200, "application/json", "{\"success\":true}");
        } else {
            request->send(500, "application/json", "{\"error\":\"System context not available\"}");
        }
    });

    // Sleep endpoint - force machine into standby mode
    server_->on("/api/sleep", HTTP_POST, [this](AsyncWebServerRequest* request) {
        if (systemContext_) {
            requestStandby(systemContext_);
            LOG(INFO, "Standby requested via API");
            request->send(200, "application/json", "{\"success\":true}");
        } else {
            request->send(500, "application/json", "{\"error\":\"System context not available\"}");
        }
    });

    // Steam control endpoints
    server_->on("/api/steam", HTTP_POST, [this](AsyncWebServerRequest* request) {
        try {
            if (!isMachineStateReady(systemContext_)) {
                request->send(503, "application/json", ApiResponses::errorResponse("Machine not ready"));
                return;
            }

            const bool steamMode = !systemContext_->machineStateContext()->isSteamModeActive();
            systemContext_->machineStateContext()->setSteamModeActive(steamMode);
            systemContext_->standbyCoordinator().reset();
            requestNormalOperation(systemContext_);
            LOGF(INFO,
                 "Toggle steam mode: %s",
                 systemContext_->machineStateContext()->isSteamModeActive() ? "on" : "off");
            request->send(
                200,
                "application/json",
                ApiResponses::boolResponse("steamMode", systemContext_->machineStateContext()->isSteamModeActive()));
        } catch (const std::exception& e) {
            LOGF(ERROR, "API steam toggle failed: %s", e.what());
            request->send(500, "application/json", ApiResponses::errorResponse("Internal server error"));
        }
    });

    // PID control endpoints
    server_->on("/api/pid", HTTP_POST, [this](AsyncWebServerRequest* request) {
        try {
            LOGF(INFO, "/api/pid requested, method: %d", request->method());

            const bool currentPidState = Config::getInstance().pidEnabled.get();
            const bool newPidState     = !currentPidState;
            (void)Config::getInstance().pidEnabled.set(newPidState);
            if (systemContext_) {
                systemContext_->setProcessPidEnabled(newPidState);
                systemContext_->standbyCoordinator().reset();
                requestNormalOperation(systemContext_);
            }

            LOGF(INFO, "Toggle PID state: %d", newPidState);

            request->send(200, "application/json", ApiResponses::boolResponse("pidEnabled", newPidState));
        } catch (const std::exception& e) {
            LOGF(ERROR, "API PID toggle failed: %s", e.what());
            request->send(500, "application/json", ApiResponses::errorResponse("Internal server error"));
        }
    });

    // Backflush endpoint
    server_->on("/api/backflush", HTTP_POST, [this](AsyncWebServerRequest* request) {
        try {
            if (!isMachineStateReady(systemContext_)) {
                request->send(503, "application/json", ApiResponses::errorResponse("Machine not ready"));
                return;
            }

            systemContext_->setBackflushMode(!systemContext_->backflushMode());
            const bool backflushOn = systemContext_->backflushMode();
            LOGF(INFO, "Toggle backflush mode: %s", backflushOn ? "on" : "off");

            JsonDocument doc;
            doc["success"]     = true;
            doc["backflushOn"] = backflushOn;

            String response;
            serializeJson(doc, response);
            request->send(200, "application/json", response);
        } catch (const std::exception& e) {
            LOGF(ERROR, "API backflush failed: %s", e.what());
            request->send(500, "application/json", ApiResponses::errorResponse("Internal server error"));
        }
    });

    server_->on("/api/maintenance/reset-backflush-counter", HTTP_POST, [this](AsyncWebServerRequest* request) {
        try {
            if (!systemContext_) {
                request->send(500, "application/json", ApiResponses::errorResponse("System context not available"));
                return;
            }

            systemContext_->maintenanceCoordinator().resetSinceBackflush();
            systemContext_->standbyCoordinator().reset();

            JsonDocument doc;
            doc["success"]              = true;
            doc["shotsSinceBackflush"]  = systemContext_->maintenanceCoordinator().getShotsSinceBackflush();
            doc["backflushReminderDue"] = systemContext_->maintenanceCoordinator().isReminderDue();

            String response;
            serializeJson(doc, response);
            request->send(200, "application/json", response);
        } catch (const std::exception& e) {
            LOGF(ERROR, "API reset backflush counter failed: %s", e.what());
            request->send(500, "application/json", ApiResponses::errorResponse("Internal server error"));
        }
    });

    // Scale control endpoints
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        server_->on("/api/scale/tare", HTTP_POST, [this](AsyncWebServerRequest* request) {
            try {
                if (systemContext_) {
                    systemContext_->sensorCoordinator().setScaleTareMode(
                        !systemContext_->sensorCoordinator().isScaleTareMode());
                    systemContext_->standbyCoordinator().reset();
                    requestNormalOperation(systemContext_);
                    LOGF(INFO,
                         "Toggle scale tare mode: %s",
                         systemContext_->sensorCoordinator().isScaleTareMode() ? "on" : "off");
                    request->send(200,
                                  "application/json",
                                  ApiResponses::boolResponse("scaleTareOn",
                                                             systemContext_->sensorCoordinator().isScaleTareMode()));
                } else {
                    request->send(500, "application/json", ApiResponses::errorResponse("System context not available"));
                }
            } catch (const std::exception& e) {
                LOGF(ERROR, "API scale tare failed: %s", e.what());
                request->send(500, "application/json", ApiResponses::errorResponse("Internal server error"));
            }
        });

        server_->on("/api/scale/calibration", HTTP_POST, [this](AsyncWebServerRequest* request) {
            try {
                if (systemContext_) {
                    systemContext_->sensorCoordinator().setScaleCalibrationMode(
                        !systemContext_->sensorCoordinator().isScaleCalibrationMode());
                    systemContext_->standbyCoordinator().reset();
                    requestNormalOperation(systemContext_);
                    LOGF(INFO,
                         "Toggle scale calibration mode: %s",
                         systemContext_->sensorCoordinator().isScaleCalibrationMode() ? "on" : "off");
                    request->send(
                        200,
                        "application/json",
                        ApiResponses::boolResponse("scaleCalibrationOn",
                                                   systemContext_->sensorCoordinator().isScaleCalibrationMode()));
                } else {
                    request->send(500, "application/json", ApiResponses::errorResponse("System context not available"));
                }
            } catch (const std::exception& e) {
                LOGF(ERROR, "API scale calibration failed: %s", e.what());
                request->send(500, "application/json", ApiResponses::errorResponse("Internal server error"));
            }
        });
    }

    // Config endpoint - reset standby on config changes
    server_->on("/api/config", HTTP_POST, [this](AsyncWebServerRequest* request) {
        if (request->hasParam("body", true)) {
            String body = request->getParam("body", true)->value();
            if (Config::getInstance().importFromJson(body)) {
                if (systemContext_) {
                    systemContext_->standbyCoordinator().reset();
                    requestNormalOperation(systemContext_);
                }
                request->send(200, "application/json", "{\"success\":true}");
            } else {
                request->send(400, "application/json", "{\"error\":\"Invalid configuration\"}");
            }
        } else {
            request->send(400, "application/json", "{\"error\":\"No body provided\"}");
        }
    });

    // Parameter help endpoint
    server_->on("/api/parameter-help", HTTP_GET, [](AsyncWebServerRequest* request) {
        try {
            const auto* p = request->getParam("param");
            if (p == nullptr) {
                request->send(422, "application/json", "{\"error\":\"parameter is missing\"}");
                return;
            }

            const String& paramName = p->value();
            BaseParamDef* paramDef  = Config::getInstance().findConfigParameter(paramName);

            if (paramDef == nullptr) {
                request->send(404, "application/json", "{\"error\":\"parameter not found\"}");
                return;
            }

            JsonDocument doc;
            doc["name"]     = paramName;
            doc["helpText"] = paramDef->getHelpText();

            String helpJson;
            if (serializeJson(doc, helpJson) == 0) {
                request->send(500, "application/json", "{\"error\":\"Failed to serialize help data\"}");
                return;
            }

            request->send(200, "application/json", helpJson);
        } catch (const std::exception& e) {
            LOGF(ERROR, "API parameter-help failed: %s", e.what());
            request->send(500, "application/json", "{\"error\":\"Help data unavailable\"}");
        }
    });

    // Temperature data endpoint
    server_->on("/api/temperatures", HTTP_GET, [this](AsyncWebServerRequest* request) {
        try {
            String tempJson = getTempString();
            request->send(200, "application/json", tempJson);
        } catch (const std::exception& e) {
            LOGF(ERROR, "API temperatures failed: %s", e.what());
            request->send(500, "application/json", "{\"error\":\"Internal server error\"}");
        }
    });

    // Temperature history endpoint
    server_->on("/api/history", HTTP_GET, [](AsyncWebServerRequest* request) {
        try {
            logMemoryUsage("handleTimeseries start");

            JsonDocument doc;
            tempHistory.generateJson(doc);

            if (doc.overflowed()) {
                request->send(500, "application/json", ApiResponses::errorResponse("timeseries JSON overflowed"));
                return;
            }

            String json;
            if (!safeSerializeJson(doc, json)) {
                request->send(
                    500, "application/json", ApiResponses::errorResponse("Failed to serialize timeseries data"));
                return;
            }

            request->send(200, "application/json", json);
        } catch (const std::exception& e) {
            LOGF(ERROR, "API history failed: %s", e.what());
            request->send(500, "application/json", ApiResponses::errorResponse("Timeseries data unavailable"));
        }
    });

    // NVS debug endpoint
    server_->on("/api/nvs-debug", HTTP_GET, [](AsyncWebServerRequest* request) {
        try {
            JsonDocument doc;
            JsonObject   nvsData  = doc.to<JsonObject>();
            JsonObject   metadata = nvsData["metadata"].to<JsonObject>();

            Preferences prefs;
            prefs.begin("config", true); // Read-only mode - use correct namespace

            // Get basic NVS information
            JsonArray allParamsArray = doc["parameters"].to<JsonArray>();
            Config::getInstance().getAllParameters(allParamsArray, "all");

            metadata["total_parameters"] = allParamsArray.size();
            metadata["nvs_namespace"]    = "config";
            metadata["free_heap"]        = ESP.getFreeHeap();
            metadata["min_free_heap"]    = ESP.getMinFreeHeap();

            nvsData["message"]          = "NVS debugging - parameter details available";
            nvsData["parameters_count"] = allParamsArray.size();

            prefs.end();

            String debugJson;
            if (serializeJson(doc, debugJson) == 0) {
                request->send(500, "application/json", "{\"error\":\"Failed to serialize NVS debug data\"}");
                return;
            }

            LOGF(INFO, "NVS Debug: basic info returned for %d parameters", allParamsArray.size());
            request->send(200, "application/json", debugJson);
        } catch (const std::exception& e) {
            LOGF(ERROR, "API nvs-debug failed: %s", e.what());
            request->send(500, "application/json", "{\"error\":\"Internal server error\"}");
        }
    });

    // WiFi reset endpoint
    server_->on("/api/wifi-reset", HTTP_POST, [this](AsyncWebServerRequest* request) {
        try {
            JsonDocument doc;
            doc["success"] = true;
            doc["message"] = "WiFi settings are being reset. Rebooting...";

            String response;
            serializeJson(doc, response);
            request->send(200, "application/json", response);

            delay(1000);

            if (systemContext_ && systemContext_->cleverCoffeeWiFiManager()) {
                systemContext_->cleverCoffeeWiFiManager()->resetSettings();
            } else {
                LOG(ERROR, "WiFiManager not initialized for reset");
                ESP.restart();
            }

        } catch (const std::exception& e) {
            LOGF(ERROR, "API wifi-reset failed: %s", e.what());
            request->send(500, "application/json", "{\"error\":\"WiFi reset failed\"}");
        }
    });

    // Config download endpoint
    server_->on("/api/config/download", HTTP_GET, [](AsyncWebServerRequest* request) {
        try {
            // Generate JSON config from current parameter values using exportToJson
            String configJson = Config::getInstance().exportToJson();

            if (configJson.isEmpty()) {
                request->send(500, "application/json", "{\"error\":\"Failed to generate config\"}");
                return;
            }

            // Prettify the JSON for readable export
            JsonDocument               doc;
            const DeserializationError error = deserializeJson(doc, configJson);

            if (error) {
                request->send(500, "application/json", "{\"error\":\"Failed to parse generated config\"}");
                return;
            }

            String prettifiedJson;
            serializeJsonPretty(doc, prettifiedJson);

            AsyncWebServerResponse* response = request->beginResponse(200, "application/json", prettifiedJson);
            response->addHeader("Content-Disposition", "attachment; filename=\"config.json\"");
            request->send(response);
        } catch (const std::exception& e) {
            LOGF(ERROR, "API config download failed: %s", e.what());
            request->send(500, "application/json", "{\"error\":\"Config download failed\"}");
        }
    });

    // Config upload endpoint with file upload handler
    server_->on(
        "/api/config/upload",
        HTTP_POST,
        [](AsyncWebServerRequest* request) {
            // Response handled by upload handler
        },
        [](AsyncWebServerRequest* request,
           const String&          filename,
           size_t                 index,
           uint8_t*               data,
           size_t                 len,
           bool                   final) {
            try {
                static String uploadBuffer;
                static size_t totalSize = 0;

                // Maximum config upload size: 16KB to prevent memory exhaustion
                static constexpr size_t MAX_CONFIG_UPLOAD_SIZE = 16384;

                if (index == 0) {
                    uploadBuffer = "";
                    uploadBuffer.reserve(8192);
                    totalSize = 0;
                    LOGF(INFO, "Config upload started: %s", filename.c_str());
                }

                totalSize += len;

                if (totalSize > MAX_CONFIG_UPLOAD_SIZE) {
                    LOGF(ERROR, "Config upload rejected: size %u exceeds limit %u", totalSize, MAX_CONFIG_UPLOAD_SIZE);
                    uploadBuffer = "";
                    request->send(413,
                                  "application/json",
                                  R"({"success": false, "message": "Config file too large. Maximum size is 16KB."})");
                    return;
                }

                for (size_t i = 0; i < len; i++) {
                    uploadBuffer += static_cast<char>(data[i]);
                }

                if (final) {
                    LOGF(INFO, "Config upload finished: %s, total size: %u bytes", filename.c_str(), totalSize);

                    if (bool isValid = Config::getInstance().importFromJson(uploadBuffer)) {
                        LOG(INFO, "Configuration validated and applied successfully");

                        AsyncWebServerResponse* response = request->beginResponse(
                            200,
                            "application/json",
                            R"({"success": true, "message": "Configuration validated and applied successfully.", "restart": true})");

                        response->addHeader("Connection", "close");
                        request->send(response);
                    } else {
                        LOG(ERROR, "Configuration validation failed - invalid data or out of range values");

                        AsyncWebServerResponse* response = request->beginResponse(
                            400,
                            "application/json",
                            R"({"success": false, "message": "Configuration validation failed. Please check that all parameter values are within valid ranges.", "restart": true})");

                        response->addHeader("Connection", "close");
                        request->send(response);
                    }
                }
            } catch (const std::exception& e) {
                LOGF(ERROR, "Config upload failed: %s", e.what());
                request->send(500, "application/json", "{\"error\":\"Upload failed\"}");
            }
        });

    // System restart endpoint
    server_->on("/api/restart", HTTP_POST, [](AsyncWebServerRequest* request) {
        try {
            JsonDocument doc;
            doc["success"] = true;
            doc["message"] = "Restarting...";

            String response;
            serializeJson(doc, response);
            request->send(200, "application/json", response);

            delay(100);
            ESP.restart();
        } catch (const std::exception& e) {
            LOGF(ERROR, "API restart failed: %s", e.what());
            request->send(500, "application/json", "{\"error\":\"Restart failed\"}");
        }
    });

    // Factory reset endpoint
    server_->on("/api/factory-reset", HTTP_POST, [](AsyncWebServerRequest* request) {
        try {
            // Reset all parameters to defaults using unified config system
            Config::getInstance().resetAllToDefaults();

            // Clear NVS preferences
            Preferences prefs;
            prefs.begin("config", false);
            bool cleared = prefs.clear();
            prefs.end();

            JsonDocument doc;
            doc["success"] = true;
            doc["message"] = cleared ? "Factory reset. Restarting..." : "Could not clear preferences. Restarting...";

            String response;
            serializeJson(doc, response);
            request->send(200, "application/json", response);

            delay(100);
            ESP.restart();
        } catch (const std::exception& e) {
            LOGF(ERROR, "API factory-reset failed: %s", e.what());
            request->send(500, "application/json", "{\"error\":\"Factory reset failed\"}");
        }
    });

    // Parameters endpoint (ANY method)
    server_->on("/api/parameters", HTTP_ANY, [this](AsyncWebServerRequest* request) {
        try {
            if (request->method() == HTTP_GET) {
                // Return all parameters
                JsonDocument doc;
                JsonArray    parametersArray = doc.to<JsonArray>();
                Config::getInstance().getAllParameters(parametersArray, "all");

                String json;
                if (serializeJson(doc, json) == 0) {
                    request->send(500, "application/json", "{\"error\":\"Failed to serialize parameters\"}");
                    return;
                }

                request->send(200, "application/json", json);
            } else if (request->method() == HTTP_POST) {
                // Update parameters from form data
                int requestParams = request->params();
                LOGF(INFO, "handleParameters POST: Received %d parameters", requestParams);

                bool hasErrors  = false;
                bool hasUpdates = false;

                for (auto i = 0u; i < requestParams; ++i) {
                    if (const auto* p = request->getParam(i); p && p->name().length() > 0 && p->value().length() > 0) {
                        const String& varName = p->name();
                        const String& value   = p->value();

                        LOGF(INFO,
                             "handleParameters POST: Processing parameter '%s' = '%s'",
                             varName.c_str(),
                             value.c_str());

                        try {
                            // Use the new fromString method for clean type conversion
                            ConfigParamDef* paramDef = Config::getInstance().findConfigParameter(varName);

                            if (paramDef) {
                                bool updateSuccess = paramDef->fromString(value);

                                if (updateSuccess) {
                                    hasUpdates = true;
                                    LOGF(INFO,
                                         "handleParameters POST: Successfully updated and saved parameter '%s' to '%s'",
                                         varName.c_str(),
                                         value.c_str());
                                } else {
                                    LOGF(WARNING, "Failed to update parameter '%s'", varName.c_str());
                                    hasErrors = true;
                                }
                            } else {
                                LOGF(WARNING, "Parameter '%s' not found", varName.c_str());
                                hasErrors = true;
                            }
                        } catch (const std::exception& e) {
                            LOGF(ERROR, "Error processing parameter '%s': %s", varName.c_str(), e.what());
                            hasErrors = true;
                        }
                    }
                }

                if (hasErrors) {
                    request->send(400, "application/json", "{\"error\":\"Some parameter updates failed\"}");
                } else if (hasUpdates) {
                    if (systemContext_) {
                        systemContext_->standbyCoordinator().reset();
                        requestNormalOperation(systemContext_);
                    }
                    request->send(
                        200, "application/json", "{\"success\":true,\"message\":\"Parameters updated and saved\"}");
                } else {
                    request->send(200, "application/json", "{\"success\":true,\"message\":\"No parameters updated\"}");
                }
            } else {
                request->send(405, "application/json", "{\"error\":\"Method not allowed\"}");
            }
        } catch (const std::exception& e) {
            LOGF(ERROR, "API parameters failed: %s", e.what());
            request->send(500, "application/json", "{\"error\":\"Parameter processing failed\"}");
        }
    });

    LOG(INFO, "API routes setup complete");
}

#if !FRONTEND_PREPROCESSING
bool WebServerManager::serveGzippedFile(AsyncWebServerRequest* request, const String& path) {
    LOGF(INFO, "Request URL: %s -> Serving path: %s", request->url().c_str(), path.c_str());

    char gzipPath[PATH_BUFFER_SIZE];
    snprintf(gzipPath, sizeof(gzipPath), "%s.gz", path.c_str());

    if (LittleFS.exists(gzipPath)) {
        LOGF(INFO, "Serving gzipped file: %s", gzipPath);

        AsyncWebServerResponse* response = request->beginResponse(LittleFS, gzipPath, getContentType(path));
        response->addHeader("Content-Encoding", "gzip");

        // Don't cache index.html - cache everything else for 7 days
        if (path.endsWith("index.html")) {
            response->addHeader("Cache-Control", "no-cache, no-store, must-revalidate");
            response->addHeader("Pragma", "no-cache");
            response->addHeader("Expires", "0");
            LOGF(DEBUG, "No-cache headers added for index.html");
        } else {
            response->addHeader("Cache-Control", "max-age=604800"); // 7 days cache for assets
        }

        request->send(response);
        return true;
    }

    LOGF(INFO, "Gzipped file not found, trying uncompressed: %s", path.c_str());

    if (LittleFS.exists(path)) {
        LOGF(DEBUG, "Serving uncompressed file: %s", path.c_str());

        AsyncWebServerResponse* response = request->beginResponse(LittleFS, path, getContentType(path));

        // Apply same caching logic for uncompressed files
        if (path.endsWith("index.html")) {
            response->addHeader("Cache-Control", "no-cache, no-store, must-revalidate");
            response->addHeader("Pragma", "no-cache");
            response->addHeader("Expires", "0");
        } else {
            response->addHeader("Cache-Control", "max-age=604800");
        }

        request->send(response);
        return true;
    }

    LOGF(INFO, "File not found: %s", path.c_str());
    return false;
}
#endif

void WebServerManager::setupStaticRoutes() {
    if (!littleFSAvailable_) {
        LOG(WARNING, "LittleFS not available, skipping static routes");
        return;
    }

    LOG(DEBUG, "Setting up static file routes");

#if FRONTEND_PREPROCESSING
    server_->serveStatic("/css", LittleFS, "/css/", "max-age=604800"); // cache for one week
    server_->serveStatic("/js", LittleFS, "/js/", "max-age=604800");
    server_->serveStatic("/img", LittleFS, "/img/", "max-age=604800"); // cache for one week
    server_->serveStatic("/webfonts", LittleFS, "/webfonts/", "max-age=604800");
    server_->serveStatic("/manifest.json", LittleFS, "/manifest.json", "max-age=604800");
    server_->serveStatic("/", LittleFS, "/html/", "max-age=604800")
        .setDefaultFile("index.html")
        .setTemplateProcessor(staticProcessor);
#else
    server_->on("/", HTTP_GET, [](AsyncWebServerRequest* request) { request->redirect("/ui/"); });

    // handles all /ui paths
    server_->on("/ui", HTTP_GET, [this](AsyncWebServerRequest* request) {
        String path = request->url();
        LOGF(INFO, "UI request for: %s", path.c_str());

        // For SPA routes without file extensions, serve index.html
        if (path.indexOf('.') == -1) {
            LOGF(INFO, "Serving index.html for SPA route: %s", path.c_str());

            if (serveGzippedFile(request, "/ui/index.html")) {
                return;
            }

            if (LittleFS.exists("/ui/index.html")) {
                request->send(LittleFS, "/ui/index.html", "text/html");
                return;
            }

            request->send(404, "text/plain", "index.html not found");
            return;
        }

        // For files with extensions, try to serve them (gzipped first)
        if (serveGzippedFile(request, path)) {
            return;
        }

        request->send(404, "text/plain", "File not found");
    });
#endif

    LOG(INFO, "Static file routes setup complete");
}

void WebServerManager::handleNotFound(AsyncWebServerRequest* request) {
    LOGF(DEBUG, "404 Not Found: %s", request->url().c_str());

    String path = request->url();

    if (path.startsWith("/api/")) {
        request->send(404, "application/json", ApiResponses::errorResponse("API endpoint not found"));
        return;
    }

#if !FRONTEND_PREPROCESSING
    if (path.startsWith("/ui/") && path.indexOf('.') == -1) {
        if (serveGzippedFile(request, "/ui/index.html")) {
            return;
        }

        if (LittleFS.exists("/ui/index.html")) {
            request->send(LittleFS, "/ui/index.html", "text/html");
            return;
        }

        request->send(404, "text/plain", "UI files not found");
        return;
    }
#endif

    request->send(404, "text/plain", "Not found");
}

#if FRONTEND_PREPROCESSING
String WebServerManager::templateProcessor(const String& var) {
    // Process template variables for HTML files
    if (var == "HOSTNAME") {
        return Config::getInstance().systemHostname.get();
    } else if (var == "VERSION") {
        return "CleverCoffee v2.0";
    } else if (var == "TEMP") {
        if (systemContext_ && systemContext_->processController()) {
            return String(systemContext_->processController()->getCurrentTemperature(), 1);
        }
        return String("0.0");
    } else if (var == "SETPOINT") {
        if (systemContext_ && systemContext_->processController()) {
            return String(systemContext_->processController()->getSetpoint(), 1);
        }
        return String("0.0");
    } else if (var == "UPTIME") {
        unsigned long uptimeMillis  = millis();
        unsigned long uptimeSeconds = uptimeMillis / 1000;
        unsigned long days          = uptimeSeconds / 86400;
        unsigned long hours         = (uptimeSeconds % 86400) / 3600;
        unsigned long minutes       = (uptimeSeconds % 3600) / 60;
        unsigned long seconds       = uptimeSeconds % 60;

        char uptime_buffer[32];
        snprintf(uptime_buffer, sizeof(uptime_buffer), "%lud %luh %lum %lus", days, hours, minutes, seconds);
        return String(uptime_buffer);
    }

    return String(); // Return empty string if variable not found
}

String WebServerManager::getValue(const String& varName) {
    try {
        // Use unified config system to get parameter values
        // This is a simplified approach - for now return empty string
        // In practice, you'd need to determine the parameter type and call the appropriate get<T>()
        LOGF(DEBUG, "getValue called for parameter: %s", varName.c_str());
        return "";
    } catch (const std::exception& e) {
        LOGF(ERROR, "getValue failed for %s: %s", varName.c_str(), e.what());
        return "(error retrieving " + varName + ")";
    }
}

String WebServerManager::getHeader(const String& varName) {
    static const std::unordered_map<std::string, const char*> headers = {
        {     "FONTAWESOME",R"(<link href="/css/fontawesome-6.2.1.min.css" rel="stylesheet">)"                            },
        {       "BOOTSTRAP",                  R"(<link href="/css/bootstrap-5.2.3.min.css" rel="stylesheet">)"},
        {"BOOTSTRAP_BUNDLE",                     "<script src=\"/js/bootstrap.bundle.5.2.3.min.js\"></script>"},
        {           "VUEJS",                                 "<script src=\"/js/vue.3.2.47.min.js\"></script>"},
        {"VUE_NUMBER_INPUT",                           "<script src=\"/js/vue-number-input.min.js\"></script>"},
        {           "UPLOT",
         R"(<script src="/js/uPlot.1.6.28.min.js"></script><link rel="stylesheet" href="/css/uPlot.min.css">)"}
    };

    const auto it = headers.find(varName.c_str());
    return it != headers.end() ? String(it->second) : String("");
}

String WebServerManager::staticProcessor(const String& var) {
    try {
        if (var.startsWith("VAR_SHOW_")) {
            return getValue(var.substring(9));
        }

        if (var.startsWith("VAR_HEADER_")) {
            return getHeader(var.substring(11));
        }

        static String fragmentPath;
        fragmentPath  = "/html_fragments/";
        fragmentPath += var;
        fragmentPath.toLowerCase();
        fragmentPath += ".html";

        if (File file = LittleFS.open(fragmentPath, "r")) {
            if (file.size() * 2 < ESP.getFreeHeap()) {
                String ret = file.readString();
                file.close();
                return ret;
            }
            file.close();
            LOGF(INFO, "Can't open file %s, not enough memory available", fragmentPath.c_str());
        } else {
            LOGF(INFO, "Fragment %s not found", fragmentPath.c_str());
        }

        return String();
    } catch (const std::exception& e) {
        LOGF(ERROR, "staticProcessor failed for %s: %s", var.c_str(), e.what());
        return String();
    }
}
#endif

void WebServerManager::sendTempEvent(double currentTemp, double targetTemp, double heaterPower) {
    if (!isRunning_ || !events_) {
        return;
    }

    // Add to temperature history
    tempHistory.addPoint(currentTemp, targetTemp, heaterPower);

    // JsonDocument doc;
    // doc["temperature"] = currentTemp;
    // doc["setpoint"] = targetTemp;
    // doc["heaterPower"] = heaterPower;

    // String data;
    // serializeJson(doc, data);

    events_->send("ping", nullptr, millis());
    sendEvent("new_temps", getTempString());
}

void WebServerManager::sendWeightEvent() {
    if (!isRunning_ || !events_) {
        return;
    }

    String weightJson = getWeightJsonString();
    sendEvent("weight", weightJson);
}

void WebServerManager::sendEvent(const String& event, const String& data) {
    if (!isRunning_ || !events_) {
        return;
    }

    events_->send(data.c_str(), event.c_str(), millis());
}

String WebServerManager::getTempString() const {
    try {
        JsonDocument doc;

        if (!systemContext_ || !systemContext_->processController()) {
            return String("{\"error\":\"System context not available\"}");
        }

        doc["currentTemp"] = systemContext_->processController()->getCurrentTemperature();
        doc["targetTemp"]  = Config::getInstance().brewSetpoint.get();
        doc["heaterPower"] = systemContext_->processController()->getPIDOutput() / 10;

        String json;
        if (!safeSerializeJson(doc, json)) {
            return "{\"error\": \"Failed to serialize temperature data\"}";
        }
        return json;
    } catch (const std::exception& e) {
        LOGF(ERROR, "getTempString failed: %s", e.what());
        return "{\"error\": \"Temperature data unavailable\"}";
    }
}

String WebServerManager::getWeightJsonString() const {
    try {
        JsonDocument doc;

        if (!systemContext_) {
            return "{\"error\": \"System context not available\"}";
        }

        doc["weight"]     = round2(systemContext_->sensorCoordinator().getWeight());
        doc["brewWeight"] = round2(systemContext_->sensorCoordinator().getBrewWeight());

        String json;
        if (!safeSerializeJson(doc, json)) {
            return "{\"error\": \"Failed to serialize weight data\"}";
        }
        return json;
    } catch (const std::exception& e) {
        LOGF(ERROR, "getWeightJsonString failed: %s", e.what());
        return "{\"error\": \"Weight data unavailable\"}";
    }
}
