/**
 * @file embeddedWebserver.h
 * @brief Embedded webserver with gzip compression support
 */

#pragma once

#include "FS.h"
#include "LittleFS.h"
#include "ota.h"
#include "state/GlobalState.h"
#include "utils/helperUtils.h"
#include "utils/legacyUtils.h"
#include <Arduino.h>
#include <ArduinoJson.h>
#include <AsyncJson.h>
#include <AsyncTCP.h>
#include <ESPAsyncWebServer.h>
#include <Preferences.h>
#include <WiFi.h>
#include <unordered_map>

inline AsyncWebServer server(80);
inline AsyncEventSource events("/events");
AsyncCorsMiddleware corsMiddleware;
AsyncAuthenticationMiddleware authMiddleware;

#define JSON_BUFFER_SIZE 512
#define PATH_BUFFER_SIZE 128
// #define RESPONSE_BUFFER_SIZE 256

// Memory monitoring function
void logMemoryUsage(const char* location) {
    size_t freeHeap = ESP.getFreeHeap();
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

// ==================== JSON RESPONSE BUILDER ====================

class JsonResponseBuilder {
    private:
        static char buffer[JSON_BUFFER_SIZE];

    public:
        static const char* createBoolResponse(const char* key, bool value, bool success = true) {
            snprintf(buffer, sizeof(buffer), "{\"success\": %s, \"%s\": %s}", success ? "true" : "false", key, value ? "true" : "false");
            return buffer;
        }

        static const char* createErrorResponse(const char* message) {
            snprintf(buffer, sizeof(buffer), "{\"error\": \"%s\"}", message);
            return buffer;
        }

        static const char* createSuccessResponse(const char* message) {
            snprintf(buffer, sizeof(buffer), "{\"success\": true, \"message\": \"%s\"}", message);
            return buffer;
        }
};

char JsonResponseBuilder::buffer[512];

// ==================== TEMPERATURE HISTORY ====================

class TemperatureHistory {
    private:
        static constexpr size_t HISTORY_SIZE = 600;
        static constexpr size_t SKIP_INTERVAL = 2;

        struct HistoryPoint {
                float currentTemp;
                float targetTemp;
                float heaterPower;
        };

        HistoryPoint history[HISTORY_SIZE];
        size_t currentIndex = 0;
        size_t valueCount = 0;
        size_t skipCounter = 0;

    public:
        void addPoint(double currentTemp, double targetTemp, double heaterPower) {
            if (++skipCounter <= SKIP_INTERVAL) return;

            skipCounter = 0;
            history[currentIndex] = {static_cast<float>(currentTemp), static_cast<float>(targetTemp), static_cast<float>(heaterPower)};

            currentIndex = (currentIndex + 1) % HISTORY_SIZE;
            if (valueCount < HISTORY_SIZE) valueCount++;
        }

        void generateJson(JsonDocument& doc) const {
            auto currentTemps = doc["currentTemps"].to<JsonArray>();
            auto targetTemps = doc["targetTemps"].to<JsonArray>();
            auto heaterPowers = doc["heaterPowers"].to<JsonArray>();

            size_t startIdx = (currentIndex >= valueCount) ? (currentIndex - valueCount) : (HISTORY_SIZE - (valueCount - currentIndex));

            for (size_t i = 0; i < valueCount; i++) {
                size_t idx = (startIdx + i) % HISTORY_SIZE;
                const auto& point = history[idx];

                currentTemps.add(round2(point.currentTemp));
                targetTemps.add(round2(point.targetTemp));
                heaterPowers.add(round2(point.heaterPower));
            }
        }
};

inline TemperatureHistory tempHistory;

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

#if !FRONTEND_PREPROCESSING
inline bool serveGzippedFile(AsyncWebServerRequest* request, const String& path) {
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
        }
        else {
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
        }
        else {
            response->addHeader("Cache-Control", "max-age=604800");
        }

        request->send(response);
        return true;
    }

    LOGF(INFO, "File not found: %s", path.c_str());
    return false;
}
#endif

// ==================== DATA RETRIEVAL FUNCTIONS ====================

inline String getTempString() {
    try {
        JsonDocument doc;

        doc["currentTemp"] = g_state.process.temperature;
        doc["targetTemp"] = Config::getInstance().brewSetpoint.get();
        doc["heaterPower"] = g_state.process.pidOutput / 10;

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

inline String getWeightJsonString() {
    try {
        JsonDocument doc;

        doc["weight"] = round2(g_state.sensors.currReadingWeight);
        doc["brewWeight"] = round2(g_state.sensors.currBrewWeight);

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

#if FRONTEND_PREPROCESSING
inline String getValue(const String& varName) {
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

// ==================== TEMPLATE PROCESSOR ====================

inline String getHeader(const String& varName) {
    static const std::unordered_map<std::string, const char*> headers = {
        {"FONTAWESOME", R"(<link href="/css/fontawesome-6.2.1.min.css" rel="stylesheet">)"}, {"BOOTSTRAP", R"(<link href="/css/bootstrap-5.2.3.min.css" rel="stylesheet">)"},
        {"BOOTSTRAP_BUNDLE", "<script src=\"/js/bootstrap.bundle.5.2.3.min.js\"></script>"}, {"VUEJS", "<script src=\"/js/vue.3.2.47.min.js\"></script>"},
        {"VUE_NUMBER_INPUT", "<script src=\"/js/vue-number-input.min.js\"></script>"},       {"UPLOT", R"(<script src="/js/uPlot.1.6.28.min.js"></script><link rel="stylesheet" href="/css/uPlot.min.css">)"}};

    const auto it = headers.find(varName.c_str());
    return it != headers.end() ? String(it->second) : String("");
}

inline String staticProcessor(const String& var) {
    try {
        if (var.startsWith("VAR_SHOW_")) {
            return getValue(var.substring(9));
        }

        if (var.startsWith("VAR_HEADER_")) {
            return getHeader(var.substring(11));
        }

        static String fragmentPath;
        fragmentPath = "/html_fragments/";
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
        }
        else {
            LOGF(INFO, "Fragment %s not found", fragmentPath.c_str());
        }

        return String();
    } catch (const std::exception& e) {
        LOGF(ERROR, "staticProcessor failed for %s: %s", var.c_str(), e.what());
        return String();
    }
}
#endif

inline void handleToggleSteam(AsyncWebServerRequest* request) {
    try {
        const bool steamMode = !g_state.machine.steamON;
        setSteamMode(steamMode);
        LOGF(INFO, "Toggle steam mode: %s", g_state.machine.steamON ? "on" : "off");
        request->send(200, "application/json", JsonResponseBuilder::createBoolResponse("steamMode", g_state.machine.steamON));
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleToggleSteam failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Internal server error"));
    }
}

inline void handleTogglePid(AsyncWebServerRequest* request) {
    try {
        LOGF(INFO, "/api/pid requested, method: %d", request->method());

        const bool currentPidState = Config::getInstance().pidEnabled.get();
        const bool newPidState = !currentPidState;
        Config::getInstance().pidEnabled.set(newPidState);

        LOGF(INFO, "Toggle PID state: %d\n", newPidState);

        request->send(200, "application/json", JsonResponseBuilder::createBoolResponse("pidEnabled", newPidState));
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleTogglePid failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Internal server error"));
    }
}

inline void handleToggleBackflush(AsyncWebServerRequest* request) {
    try {
        g_state.machine.backflushOn = !g_state.machine.backflushOn;
        LOGF(INFO, "Toggle backflush mode: %s", g_state.machine.backflushOn ? "on" : "off");
        request->send(200, "application/json", JsonResponseBuilder::createBoolResponse("g_state.machine.backflushOn", g_state.machine.backflushOn));
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleToggleBackflush failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Internal server error"));
    }
}

inline void handleToggleTareScale(AsyncWebServerRequest* request) {
    try {
        g_state.sensors.scaleTareOn = !g_state.sensors.scaleTareOn;
        LOGF(INFO, "Toggle scale tare mode: %s", g_state.sensors.scaleTareOn ? "on" : "off");
        request->send(200, "application/json", JsonResponseBuilder::createBoolResponse("g_state.sensors.scaleTareOn", g_state.sensors.scaleTareOn));
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleToggleTareScale failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Internal server error"));
    }
}

inline void handleToggleScaleCalibration(AsyncWebServerRequest* request) {
    try {
        g_state.sensors.scaleCalibrationOn = !g_state.sensors.scaleCalibrationOn;
        LOGF(INFO, "Toggle scale calibration mode: %s", g_state.sensors.scaleCalibrationOn ? "on" : "off");
        request->send(200, "application/json", JsonResponseBuilder::createBoolResponse("g_state.sensors.scaleCalibrationOn", g_state.sensors.scaleCalibrationOn));
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleToggleScaleCalibration failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Internal server error"));
    }
}

inline void handleParameters(AsyncWebServerRequest* request) {
    try {
        logMemoryUsage("handleParameters start");

        if (request->method() == 1) { // HTTP_GET
            // Use the unified Config system for API
            auto& config = Config::getInstance();

            String filterType = "";
            if (request->hasParam("filter")) {
                filterType = request->getParam("filter")->value();
            }

            AsyncJsonResponse* response = new AsyncJsonResponse(false);
            JsonArray array = response->getRoot().to<JsonArray>();

            // Use the public getAllParameters method
            Config::getInstance().getAllParameters(array, filterType);

            LOGF(INFO, "/parameters returning %d parameters", array.size());
            response->setLength();
            request->send(response);
        }
        else if (request->method() == 2) { // HTTP_POST
            auto& config = Config::getInstance();

            String responseMessage = "OK";
            bool hasErrors = false;

            const auto requestParams = request->params();
            LOGF(INFO, "handleParameters POST: Received %d parameters", requestParams);

            // Log all parameters being sent
            for (auto i = 0u; i < requestParams; ++i) {
                if (auto* p = request->getParam(i); p && p->name().length() > 0) {
                    LOGF(INFO, "handleParameters POST: Form contains '%s' = '%s'", p->name().c_str(), p->value().c_str());
                }
            }

            for (auto i = 0u; i < requestParams; ++i) {
                if (auto* p = request->getParam(i); p && p->name().length() > 0 && p->value().length() > 0) {
                    const String& varName = p->name();
                    const String& value = p->value();

                    LOGF(INFO, "handleParameters POST: Processing parameter '%s' = '%s'", varName.c_str(), value.c_str());

                    try {
                        // Use the same pattern as MQTTManager - find parameter and use fromJson
                        BaseParamDef* paramDef = Config::getInstance().findParameter(varName);

                        if (paramDef) {
                            // Create a JsonVariant with the value
                            JsonDocument tempDoc;
                            tempDoc.set(value);
                            JsonVariant valueVariant = tempDoc.as<JsonVariant>();

                            bool updateSuccess = paramDef->fromJson(valueVariant);

                            if (!updateSuccess) {
                                LOGF(WARNING, "Failed to update parameter '%s'", varName.c_str());
                                hasErrors = true;
                            }
                        } else {
                            LOGF(WARNING, "Parameter '%s' not found", varName.c_str());
                            hasErrors = true;
                        }
                    } catch (const std::exception& e) {
                        LOGF(ERROR, "Parameter %s processing failed: %s", varName.c_str(), e.what());
                        hasErrors = true;
                        continue;
                    }
                }
            }

            writeSysParamsToMQTT(true);

            AsyncWebServerResponse* response = request->beginResponse(200, "text/plain", hasErrors ? "Partial Success" : "OK");
            response->addHeader("Connection", "close");
            request->send(response);
        }
        else {
            LOGF(ERROR, "Unsupported HTTP method %d for /parameters", request->method());
            AsyncWebServerResponse* response = request->beginResponse(405, "text/plain", "Method Not Allowed");
            response->addHeader("Connection", "close");
            request->send(response);
        }
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleParameters failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Parameter processing failed"));
    }
}

inline void handleParameterHelp(AsyncWebServerRequest* request) {
    try {
        auto* p = request->getParam("param");
        if (p == nullptr) {
            request->send(422, "application/json", JsonResponseBuilder::createErrorResponse("parameter is missing"));
            return;
        }

        const String& paramName = p->value();
        BaseParamDef* paramDef = Config::getInstance().findParameter(paramName);

        if (paramDef == nullptr) {
            request->send(404, "application/json", JsonResponseBuilder::createErrorResponse("parameter not found"));
            return;
        }

        JsonDocument doc;
        doc["name"] = paramName;
        doc["helpText"] = paramDef->getHelpText();

        String helpJson;
        if (!safeSerializeJson(doc, helpJson)) {
            request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Failed to serialize help data"));
            return;
        }
        request->send(200, "application/json", helpJson);
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleParameterHelp failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Help data unavailable"));
    }
}

inline void handleTemperatures(AsyncWebServerRequest* request) {
    try {
        const String json = getTempString();
        request->send(200, "application/json", json);
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleTemperatures failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Temperature data unavailable"));
    }
}

inline void handleTimeseries(AsyncWebServerRequest* request) {
    try {
        logMemoryUsage("handleTimeseries start");

        JsonDocument doc;
        tempHistory.generateJson(doc);

        if (doc.overflowed()) {
            request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("timeseries JSON overflowed"));
            return;
        }

        String out;
        if (!safeSerializeJson(doc, out)) {
            request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Failed to serialize timeseries data"));
            return;
        }

        request->send(200, "application/json", out);
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleTimeseries failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Timeseries data unavailable"));
    }
}

inline void handleWifiReset(AsyncWebServerRequest* request) {
    try {
        request->send(200, "application/json", JsonResponseBuilder::createSuccessResponse("WiFi settings are being reset. Rebooting..."));

        delay(1000);
        wiFiReset();
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleWifiReset failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("WiFi reset failed"));
    }
}

inline void handleConfigDownload(AsyncWebServerRequest* request) {
    try {
        // Generate JSON config from current parameter values using exportToJson
        String configJson = Config::getInstance().exportToJson();

        if (configJson.isEmpty()) {
            request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Failed to generate config"));
            return;
        }

        // Prettify the JSON
        JsonDocument doc;
        const DeserializationError error = deserializeJson(doc, configJson);

        if (error) {
            request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Failed to parse generated config"));
            return;
        }

        String prettifiedJson;
        serializeJsonPretty(doc, prettifiedJson);

        AsyncWebServerResponse* response = request->beginResponse(200, "application/json", prettifiedJson);
        response->addHeader("Content-Disposition", "attachment; filename=\"config.json\"");
        request->send(response);
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleConfigDownload failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Config download failed"));
    }
}

inline void handleConfigUpload(AsyncWebServerRequest* request, const String& filename, const size_t index, const uint8_t* data, const size_t len, const bool final) {
    try {
        static String uploadBuffer;
        static size_t totalSize = 0;

        if (index == 0) {
            uploadBuffer = "";
            uploadBuffer.reserve(8192);
            totalSize = 0;
            LOGF(INFO, "Config upload started: %s", filename.c_str());
        }

        for (size_t i = 0; i < len; i++) {
            uploadBuffer += static_cast<char>(data[i]);
        }

        totalSize += len;

        if (final) {
            LOGF(INFO, "Config upload finished: %s, total size: %u bytes", filename.c_str(), totalSize);

            if (bool isValid = Config::getInstance().importFromJson(uploadBuffer)) {
                LOG(INFO, "Configuration validated and applied successfully");

                AsyncWebServerResponse* response = request->beginResponse(200, "application/json", R"({"success": true, "message": "Configuration validated and applied successfully.", "restart": true})");

                response->addHeader("Connection", "close");
                request->send(response);
            }
            else {
                LOG(ERROR, "Configuration validation failed - invalid data or out of range values");

                AsyncWebServerResponse* response =
                    request->beginResponse(400, "application/json", R"({"success": false, "message": "Configuration validation failed. Please check that all parameter values are within valid ranges.", "restart": true})");

                response->addHeader("Connection", "close");
                request->send(response);
            }
        }
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleConfigUpload failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Config upload failed"));
    }
}

inline void handleRestart(AsyncWebServerRequest* request) {
    try {
        request->send(200, "application/json", JsonResponseBuilder::createSuccessResponse("Restarting..."));
        delay(100);
        ESP.restart();
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleRestart failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Restart failed"));
    }
}

inline void handleNvsDebug(AsyncWebServerRequest* request) {
    try {
        JsonDocument doc;
        JsonObject nvsData = doc.to<JsonObject>();

        Preferences prefs;
        prefs.begin(STORAGE_NAMESPACE, true); // Read-only mode - use correct namespace

        JsonObject metadata = nvsData["metadata"].to<JsonObject>();

        // Get basic NVS information without complex parameter iteration
        // since the getAllParamDefs method is private
        JsonArray allParamsArray = doc.to<JsonArray>();
        Config::getInstance().getAllParameters(allParamsArray, "all");

        metadata["total_parameters"] = allParamsArray.size();
        metadata["nvs_namespace"] = STORAGE_NAMESPACE;
        metadata["free_heap"] = ESP.getFreeHeap();
        metadata["min_free_heap"] = ESP.getMinFreeHeap();

        // For now, provide basic info rather than detailed NVS analysis
        nvsData["message"] = "NVS debugging simplified - parameter details available via /api/parameters";
        nvsData["parameters_count"] = allParamsArray.size();

        prefs.end();

        String debugJson;
        if (!safeSerializeJson(doc, debugJson)) {
            request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Failed to serialize NVS debug data"));
            return;
        }

        LOGF(INFO, "NVS Debug: basic info returned for %d parameters", allParamsArray.size());
        request->send(200, "application/json", debugJson);

    } catch (const std::exception& e) {
        LOGF(ERROR, "handleNvsDebug failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("NVS debug failed"));
    }
}

inline void handleFactoryReset(AsyncWebServerRequest* request) {
    try {
        // Reset all parameters to defaults using unified config system
        Config::getInstance().resetAllToDefaults();

        // Clear NVS preferences
        Preferences prefs;
        prefs.begin(STORAGE_NAMESPACE, false);
        bool cleared = prefs.clear();
        prefs.end();

        request->send(200, "application/json", cleared ? JsonResponseBuilder::createSuccessResponse("Factory reset. Restarting...") : JsonResponseBuilder::createErrorResponse("Could not clear preferences. Restarting..."));

        delay(100);
        ESP.restart();
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleFactoryReset failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Factory reset failed"));
    }
}

// ==================== API ROUTES SETUP ====================

inline void setupApiRoutes() {
    bool authEnabled = Config::getInstance().systemAuthEnabled.get();

    if (authEnabled) {
        LOG(INFO, "Authentication is enabled");
        String username = Config::getInstance().systemAuthUsername.get();
        String password = Config::getInstance().systemAuthPassword.get();

        authMiddleware.setAuthType(AsyncAuthType::AUTH_DIGEST);
        authMiddleware.setRealm("CleverCoffee");
        authMiddleware.setUsername(username.c_str());
        authMiddleware.setPassword(password.c_str());
        authMiddleware.setAuthFailureMessage("Authentication failed");
        authMiddleware.generateHash(); // optimization to avoid generating the hash at each request
    }

    server.on("/api/health", HTTP_GET, [](AsyncWebServerRequest* request) { request->send(200); });

    server.on("/api/steam", HTTP_POST, handleToggleSteam);
    server.on("/api/pid", HTTP_POST, handleTogglePid);
    server.on("/api/backflush", HTTP_POST, handleToggleBackflush);

    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        server.on("/api/scale/tare", HTTP_POST, handleToggleTareScale);
        server.on("/api/scale/calibration", HTTP_POST, handleToggleScaleCalibration);
    }

    server.on("/api/parameters", HTTP_ANY, handleParameters);
    server.on("/api/parameter-help", HTTP_GET, handleParameterHelp);
    server.on("/api/temperatures", HTTP_GET, handleTemperatures);
    server.on("/api/history", HTTP_GET, handleTimeseries);
    server.on("/api/nvs-debug", HTTP_GET, handleNvsDebug);

    server.on("/api/wifi-reset", HTTP_POST, handleWifiReset);
    server.on("/api/config/download", HTTP_GET, handleConfigDownload);
    server.on(
        "/api/config/upload", HTTP_POST,
        [](AsyncWebServerRequest* request) {
            // Response handled by upload handler
        },
        handleConfigUpload);
    server.on("/api/restart", HTTP_POST, handleRestart);
    server.on("/api/factory-reset", HTTP_POST, handleFactoryReset);

    // Setup OTA endpoints
    OTA::setup(server);
}

// ==================== 404 HANDLER ====================

inline void handleNotFound(AsyncWebServerRequest* request) {
    String path = request->url();

    if (path.startsWith("/api/")) {
        request->send(404, "application/json", JsonResponseBuilder::createErrorResponse("API endpoint not found"));
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

// ==================== SERVER SETUP ====================

void serverSetup() {
    logMemoryUsage("serverSetup start");
    LOG(INFO, "Starting serverSetup - about to call setupApiRoutes()");

    setupApiRoutes();
    LOG(INFO, "setupApiRoutes() completed successfully");

#if FRONTEND_PREPROCESSING
    server.serveStatic("/css", LittleFS, "/css/", "max-age=604800"); // cache for one week
    server.serveStatic("/js", LittleFS, "/js/", "max-age=604800");
    server.serveStatic("/img", LittleFS, "/img/", "max-age=604800"); // cache for one week
    server.serveStatic("/webfonts", LittleFS, "/webfonts/", "max-age=604800");
    server.serveStatic("/manifest.json", LittleFS, "/manifest.json", "max-age=604800");
    server.serveStatic("/", LittleFS, "/html/", "max-age=604800").setDefaultFile("index.html").setTemplateProcessor(staticProcessor);
#else
    server.on("/", HTTP_GET, [](AsyncWebServerRequest* request) { request->redirect("/ui/"); });

    // handles all /ui paths
    server.on("/ui", HTTP_GET, [](AsyncWebServerRequest* request) {
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

    server.onNotFound(handleNotFound);

    events.onConnect([](AsyncEventSourceClient* client) {
        if (client->lastId()) {
            LOGF(INFO, "Reconnected, last message ID was: %u", client->lastId());
        }

        client->send("hello", nullptr, millis(), 10000);
    });

    server.addHandler(&events);
    LOG(INFO, "Added event handler to server");

    // LittleFS is now initialized earlier in SystemInitializer
    // LittleFS.begin(); // Removed - already initialized
    LOG(INFO, "Using already initialized LittleFS");

    server.addMiddleware(&corsMiddleware);
    LOG(INFO, "Added CORS middleware");
    server.addMiddleware(&authMiddleware);
    LOG(INFO, "Added auth middleware");

    server.begin();
    LOG(INFO, "Server.begin() called successfully");

    LOG(INFO, ("Server started at " + WiFi.localIP().toString()).c_str());
    logMemoryUsage("serverSetup complete");
}

void sendTempEvent(const double currentTemp, const double targetTemp, const double heaterPower) {
    try {
        tempHistory.addPoint(currentTemp, targetTemp, heaterPower);
        events.send("ping", nullptr, millis());
        events.send(getTempString().c_str(), "new_temps", millis());
    } catch (const std::exception& e) {
        LOGF(ERROR, "sendTempEvent failed: %s", e.what());
    }
}

void sendWeightEvent() {
    try {
        String weightJson = getWeightJsonString();
        events.send(weightJson.c_str(), "weight", millis());
    } catch (const std::exception& e) {
        LOGF(ERROR, "sendWeightEvent failed: %s", e.what());
    }
}
