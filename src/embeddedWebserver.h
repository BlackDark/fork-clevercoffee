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

extern float currReadingWeight;
extern float currBrewWeight;
// temperature moved to g_state.process.temperature
// pidOutput moved to g_state.process.pidOutput

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
        doc["targetTemp"] = Config::getInstance().get<double>("brew.setpoint");
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
        extern float currReadingWeight;

        JsonDocument doc;

        doc["weight"] = round2(currReadingWeight);
        doc["brewWeight"] = round2(currBrewWeight);

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

        const bool currentPidState = Config::getInstance().get<bool>("pid.enabled");
        const bool newPidState = !currentPidState;
        Config::getInstance().set<bool>("pid.enabled", newPidState);

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
            const auto& parameters = Config::getInstance().getParameters();

            String filterType = "";
            if (request->hasParam("filter")) {
                filterType = request->getParam("filter")->value();
            }

            AsyncJsonResponse* response = new AsyncJsonResponse(false);
            JsonArray array = response->getRoot().to<JsonArray>();

            int filteredParameterCount = 0;

            for (const auto& param : parameters) {
                const std::string& name = param.first;
                const ParamDef& paramDef = param.second;
                int section = paramDef.section;

                bool includeParam = false;

#if FRONTEND_PREPROCESSING
                // old style parameter condition
                if (!paramDef.showCondition()) continue;

                if (filterType == "hardware") {
                    includeParam = section >= 11 && section <= 15;
                }
                else if (filterType == "behavior") {
                    includeParam = section >= 0 && section <= 9;
                }
                else if (filterType == "other") {
                    includeParam = section == 10;
                }
                else if (filterType == "all") {
                    includeParam = true;
                }
                else {
                    includeParam = section == 0 || section == 1 || section == 10;
                }
#else
                // new style conditions. We probably do not need filtering at all
                if (filterType == "hardware") {
                    includeParam = section == 4; // Hardware section
                }
                else if (filterType == "behavior") {
                    includeParam = section >= 0 && section <= 3;
                }
                else if (filterType == "other") {
                    includeParam = section == 5 || section == 6 || section == 7;
                }
                else if (filterType == "all") {
                    includeParam = true;
                }
                else {
                    includeParam = section == 0 || section == 1 || section == 3;
                }
#endif

                if (includeParam) {
                    // Use the ParamDef's toJson method directly
                    JsonObject result = array.add<JsonObject>();
                    paramDef.toJson(result, String(name.c_str()));
                    filteredParameterCount++;
                }
            }

            LOGF(INFO, "/parameters returning %d parameters", filteredParameterCount);
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
                        // Use unified config system to update parameters
                        LOGF(INFO, "handleParameters POST: Updating parameter '%s' = '%s'", varName.c_str(), value.c_str());

                        // Try to determine parameter type and update accordingly
                        // For now, try different types until one succeeds
                        bool updateSuccess = false;

                        // Try boolean first (common case)
                        if (value == "true" || value == "false" || value == "1" || value == "0") {
                            bool boolValue = (value == "true" || value == "1");
                            updateSuccess = Config::getInstance().set<bool>(varName, boolValue);
                        }

                        // Try integer if boolean failed
                        if (!updateSuccess && isValidNumber(value) && value.indexOf('.') == -1) {
                            int intValue = value.toInt();
                            updateSuccess = Config::getInstance().set<int>(varName, intValue);
                        }

                        // Try double if integer failed
                        if (!updateSuccess && isValidNumber(value)) {
                            double doubleValue = value.toDouble();
                            updateSuccess = Config::getInstance().set<double>(varName, doubleValue);
                        }

                        // Try string if numeric types failed
                        if (!updateSuccess) {
                            updateSuccess = Config::getInstance().set<::String>(varName, value);
                        }

                        if (!updateSuccess) {
                            LOGF(WARNING, "Failed to update parameter '%s'", varName.c_str());
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
        auto& config = Config::getInstance();
        const auto& parameters = Config::getInstance().getParameters();

        auto it = parameters.find(paramName.c_str());
        if (it == parameters.end()) {
            request->send(404, "application/json", JsonResponseBuilder::createErrorResponse("parameter not found"));
            return;
        }

        JsonDocument doc;
        doc["name"] = paramName;
        doc["helpText"] = it->second.helpText;

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
        // Generate JSON config from current parameter values
        String configJson = Config::getInstance().generateJsonConfig();

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

            if (bool isValid = Config::getInstance().validateAndApplyFromJson(uploadBuffer)) {
                LOG(INFO, "Configuration validated and applied successfully");

                // Sync to global variables (NVS save already handled by validateAndApplyFromJson)
                Config::getInstance().syncGlobalVariables();

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

        // NVS cleanup is now handled by the unified config system
        // The Config system manages its own NVS keys
        LOGF(INFO, "NVS cleanup handled by unified config system");

        JsonObject storedValues = nvsData["stored_values"].to<JsonObject>();
        JsonArray missingParams = nvsData["missing_parameters"].to<JsonArray>();
        JsonObject metadata = nvsData["metadata"].to<JsonObject>();

        int totalParams = 0;
        int storedParams = 0;

        auto& config = Config::getInstance();
        const auto& parameters = Config::getInstance().getParameters();

        for (const auto& param : parameters) {
            const std::string& paramId = param.first;
            const ParamDef& paramDef = param.second;
            totalParams++;

            // Generate hashed NVS key for the parameter (same method as Config system)
            String nvsKey = Config::getInstance().generateNvsKey(paramId.c_str());

            // Check if this parameter exists in NVS using the hashed key
            bool existsInNvs = false;
            JsonVariant value;

            switch (paramDef.type) {
                case ParamType::BOOL:
                    {
                        if (prefs.isKey(nvsKey.c_str())) {
                            bool boolVal = prefs.getBool(nvsKey.c_str());
                            storedValues[paramId.c_str()]["value"] = boolVal;
                            storedValues[paramId.c_str()]["type"] = "bool";
                            storedValues[paramId.c_str()]["nvs_key"] = nvsKey;
                            existsInNvs = true;
                            storedParams++;
                        }
                        break;
                    }
                case ParamType::INT:
                case ParamType::ENUM:
                    {
                        if (prefs.isKey(nvsKey.c_str())) {
                            int intVal = prefs.getInt(nvsKey.c_str());
                            storedValues[paramId.c_str()]["value"] = intVal;
                            storedValues[paramId.c_str()]["type"] = "int";
                            storedValues[paramId.c_str()]["nvs_key"] = nvsKey;
                            existsInNvs = true;
                            storedParams++;
                        }
                        break;
                    }
                case ParamType::UINT8:
                    {
                        if (prefs.isKey(nvsKey.c_str())) {
                            uint8_t uintVal = prefs.getUChar(nvsKey.c_str());
                            storedValues[paramId.c_str()]["value"] = static_cast<int>(uintVal);
                            storedValues[paramId.c_str()]["type"] = "uint8";
                            storedValues[paramId.c_str()]["nvs_key"] = nvsKey;
                            existsInNvs = true;
                            storedParams++;
                        }
                        break;
                    }
                case ParamType::DOUBLE:
                    {
                        if (prefs.isKey(nvsKey.c_str())) {
                            double doubleVal = prefs.getDouble(nvsKey.c_str());
                            storedValues[paramId.c_str()]["value"] = doubleVal;
                            storedValues[paramId.c_str()]["type"] = "double";
                            storedValues[paramId.c_str()]["nvs_key"] = nvsKey;
                            existsInNvs = true;
                            storedParams++;
                        }
                        break;
                    }
                case ParamType::FLOAT:
                    {
                        if (prefs.isKey(nvsKey.c_str())) {
                            float floatVal = prefs.getFloat(nvsKey.c_str());
                            storedValues[paramId.c_str()]["value"] = floatVal;
                            storedValues[paramId.c_str()]["type"] = "float";
                            storedValues[paramId.c_str()]["nvs_key"] = nvsKey;
                            existsInNvs = true;
                            storedParams++;
                        }
                        break;
                    }
                case ParamType::STRING:
                    {
                        if (prefs.isKey(nvsKey.c_str())) {
                            String stringVal = prefs.getString(nvsKey.c_str());
                            storedValues[paramId.c_str()]["value"] = stringVal;
                            storedValues[paramId.c_str()]["type"] = "string";
                            storedValues[paramId.c_str()]["nvs_key"] = nvsKey;
                            existsInNvs = true;
                            storedParams++;
                        }
                        break;
                    }
            }

            if (existsInNvs) {
                // Add current global variable value for comparison
                switch (paramDef.type) {
                    case ParamType::BOOL:
                        storedValues[paramId.c_str()]["current_global"] = *static_cast<bool*>(paramDef.globalVar);
                        break;
                    case ParamType::STRING:
                        storedValues[paramId.c_str()]["current_global"] = static_cast<::String*>(paramDef.globalVar)->c_str();
                        break;
                    case ParamType::INT:
                    case ParamType::ENUM:
                        storedValues[paramId.c_str()]["current_global"] = *static_cast<int*>(paramDef.globalVar);
                        break;
                    case ParamType::UINT8:
                        storedValues[paramId.c_str()]["current_global"] = *static_cast<uint8_t*>(paramDef.globalVar);
                        break;
                    case ParamType::DOUBLE:
                        storedValues[paramId.c_str()]["current_global"] = *static_cast<double*>(paramDef.globalVar);
                        break;
                    case ParamType::FLOAT:
                        storedValues[paramId.c_str()]["current_global"] = *static_cast<float*>(paramDef.globalVar);
                        break;
                }
                storedValues[paramId.c_str()]["display_name"] = paramDef.displayName;
            }
            else {
                JsonObject missingParam = missingParams.add<JsonObject>();
                missingParam["id"] = paramId.c_str();
                missingParam["display_name"] = paramDef.displayName;
                missingParam["type"] = static_cast<int>(paramDef.type);
                missingParam["expected_nvs_key"] = nvsKey.c_str();
                switch (paramDef.type) {
                    case ParamType::BOOL:
                        missingParam["current_value"] = *static_cast<bool*>(paramDef.globalVar);
                        break;
                    case ParamType::STRING:
                        missingParam["current_value"] = static_cast<::String*>(paramDef.globalVar)->c_str();
                        break;
                    case ParamType::INT:
                    case ParamType::ENUM:
                        missingParam["current_value"] = *static_cast<int*>(paramDef.globalVar);
                        break;
                    case ParamType::UINT8:
                        missingParam["current_value"] = *static_cast<uint8_t*>(paramDef.globalVar);
                        break;
                    case ParamType::DOUBLE:
                        missingParam["current_value"] = *static_cast<double*>(paramDef.globalVar);
                        break;
                    case ParamType::FLOAT:
                        missingParam["current_value"] = *static_cast<float*>(paramDef.globalVar);
                        break;
                }
            }
        }

        prefs.end();

        // Add metadata
        metadata["total_parameters"] = totalParams;
        metadata["stored_parameters"] = storedParams;
        metadata["missing_parameters"] = totalParams - storedParams;
        metadata["storage_percentage"] = totalParams > 0 ? (storedParams * 100) / totalParams : 0;
        metadata["nvs_namespace"] = STORAGE_NAMESPACE;
        metadata["free_heap"] = ESP.getFreeHeap();
        metadata["min_free_heap"] = ESP.getMinFreeHeap();

        String debugJson;
        if (!safeSerializeJson(doc, debugJson)) {
            request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Failed to serialize NVS debug data"));
            return;
        }

        LOGF(INFO, "NVS Debug: %d/%d parameters stored in NVS", storedParams, totalParams);
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
    bool authEnabled = Config::getInstance().get<bool>("system.auth.enabled");

    if (authEnabled) {
        LOG(INFO, "Authentication is enabled");
        String username = Config::getInstance().get<::String>("system.auth.username");
        String password = Config::getInstance().get<::String>("system.auth.password");

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

    if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
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

    setupApiRoutes();

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

    LittleFS.begin();

    server.addMiddleware(&corsMiddleware);
    server.addMiddleware(&authMiddleware);

    server.begin();

    LOG(INFO, ("Server started at " + WiFi.localIP().toString()).c_str());
    logMemoryUsage("serverSetup complete");
}

inline void sendTempEvent(const double currentTemp, const double targetTemp, const double heaterPower) {
    try {
        tempHistory.addPoint(currentTemp, targetTemp, heaterPower);
        events.send("ping", nullptr, millis());
        events.send(getTempString().c_str(), "new_temps", millis());
    } catch (const std::exception& e) {
        LOGF(ERROR, "sendTempEvent failed: %s", e.what());
    }
}

inline void sendWeightEvent() {
    try {
        String weightJson = getWeightJsonString();
        events.send(weightJson.c_str(), "weight", millis());
    } catch (const std::exception& e) {
        LOGF(ERROR, "sendWeightEvent failed: %s", e.what());
    }
}
