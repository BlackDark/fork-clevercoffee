/**
 * @file embeddedWebserver.h
 *
 * @brief Embedded webserver with gzip compression support - Optimized Version
 *
 */

#pragma once

#include <Arduino.h>
#include "FS.h"
#include <AsyncTCP.h>
#include <WiFi.h>
#include <ArduinoJson.h>
#include <AsyncJson.h>
#include <ESPAsyncWebServer.h>
#include "LittleFS.h"

inline AsyncWebServer server(80);
inline AsyncEventSource events("/events");
AsyncCorsMiddleware corsMiddleware;

extern float currReadingWeight; // defined in brewHandler.h
extern float currBrewWeight;    // defined in brewHandler.h
extern double temperature;      // defined in main.cpp
extern double brewSetpoint;     // defined in main.cpp
extern double pidOutput;        // defined in main.cpp, needs to be divided by 10 for display

#define HISTORY_LENGTH 600      // 30 mins of values (20 vals/min * 60 min) = 600 (7,2kb)

void serverSetup();

// ==================== UTILITY FUNCTIONS ====================

inline uint8_t flipUintValue(const uint8_t value) {
    return value == 0 ? 1 : 0;  // Simple boolean flip
}

// proper modulo function (% is remainder, so will return negatives)
inline int mod(const int a, const int b) {
    const int r = a % b;
    return r < 0 ? r + b : r;
}

// rounds a number to 2 decimal places
// example: round(3.14159) -> 3.14
// (less characters when serialized to json)
inline double round2(const double value) {
    return std::round(value * 100.0) / 100.0;
}

// Memory monitoring function
void logMemoryUsage(const char* location) {
    size_t freeHeap = ESP.getFreeHeap();
    size_t minFreeHeap = ESP.getMinFreeHeap();

    LOGF(DEBUG, "Memory at %s - Free: %d, Min Free: %d",
         location, freeHeap, minFreeHeap);

    if (freeHeap < 10000) { // Less than 10KB free
        LOGF(WARNING, "Low memory condition detected!");
    }
}

// Input validation function
bool isValidNumber(const String& str) {
    if (str.length() == 0) return false;

    for (size_t i = 0; i < str.length(); i++) {
        if (!isdigit(str[i]) && str[i] != '.' && str[i] != '-') {
            return false;
        }
    }
    return true;
}

// ==================== OPTIMIZED CLASSES & HELPERS ====================

// Content Type Detection
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

// Pre-allocated JSON Response Builder
class JsonResponseBuilder {
private:
    static char buffer[512];

public:
    static const char* createBoolResponse(const char* key, bool value, bool success = true) {
        snprintf(buffer, sizeof(buffer),
                "{\"success\": %s, \"%s\": %s}",
                success ? "true" : "false",
                key,
                value ? "true" : "false");
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

// Optimized Temperature History Management
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
        history[currentIndex] = {
            static_cast<float>(currentTemp),
            static_cast<float>(targetTemp),
            static_cast<float>(heaterPower)
        };

        currentIndex = (currentIndex + 1) % HISTORY_SIZE;
        if (valueCount < HISTORY_SIZE) valueCount++;
    }

    void generateJson(JsonDocument& doc) const {
        auto currentTemps = doc["currentTemps"].to<JsonArray>();
        auto targetTemps = doc["targetTemps"].to<JsonArray>();
        auto heaterPowers = doc["heaterPowers"].to<JsonArray>();

        size_t startIdx = (currentIndex >= valueCount) ?
                         (currentIndex - valueCount) :
                         (HISTORY_SIZE - (valueCount - currentIndex));

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

// Authentication Cache
struct AuthCache {
    bool enabled = false;
    String username;
    String password;
    uint32_t lastUpdate = 0;
    static constexpr uint32_t CACHE_DURATION = 10000; // 10 seconds

    void refresh() {
        uint32_t now = millis();
        if (now - lastUpdate > CACHE_DURATION) {
            enabled = config.get<bool>("system.auth.enabled");
            if (enabled) {
                username = config.get<String>("system.auth.username");
                password = config.get<String>("system.auth.password");
            }
            lastUpdate = now;
        }
    }
} static authCache;

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

// ==================== OPTIMIZED HELPER FUNCTIONS ====================

// Optimized file serving function with better debugging
inline bool serveGzippedFile(AsyncWebServerRequest* request, const String& path) {
    // Add the actual request URL to debug logs
    LOGF(DEBUG, "Request URL: %s -> Serving path: %s", request->url().c_str(), path.c_str());

    char gzipPath[128];
    snprintf(gzipPath, sizeof(gzipPath), "%s.gz", path.c_str());

    if (LittleFS.exists(gzipPath)) {
        LOGF(DEBUG, "Serving gzipped file: %s", gzipPath);

        AsyncWebServerResponse* response = request->beginResponse(LittleFS, gzipPath, getContentType(path));
        response->addHeader("Content-Encoding", "gzip");
        response->addHeader("Cache-Control", "max-age=604800"); // 7 days cache
        request->send(response);
        return true;
    }

    LOGF(DEBUG, "Gzipped file not found, trying uncompressed: %s", path.c_str());

    // Fall back to uncompressed version if gzipped doesn't exist
    if (LittleFS.exists(path)) {
        LOGF(DEBUG, "Serving uncompressed file: %s", path.c_str());
        request->send(LittleFS, path, getContentType(path));
        return true;
    }

    LOGF(DEBUG, "File not found: %s", path.c_str());
    return false;
}

inline bool authenticate(AsyncWebServerRequest* request) {
    authCache.refresh();

    if (!authCache.enabled) {
        return true;
    }

    const auto clientIP = request->client()->remoteIP().toString();
    const auto requestedPath = request->url();

    if (request->authenticate(authCache.username.c_str(), authCache.password.c_str())) {
        LOGF(DEBUG, "Web auth OK: %s -> %s", clientIP.c_str(), requestedPath.c_str());
        return true;
    }

    if (request->hasHeader("Authorization")) {
        LOGF(WARNING, "Web auth FAIL: %s -> %s (wrong credentials)", clientIP.c_str(), requestedPath.c_str());
    }
    else {
        LOGF(DEBUG, "Web auth required: %s -> %s", clientIP.c_str(), requestedPath.c_str());
    }

    return false;
}

inline String getTempString() {
    try {
        JsonDocument doc;

        doc["currentTemp"] = temperature;
        doc["targetTemp"] = brewSetpoint;
        doc["heaterPower"] = pidOutput / 10;

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

inline String getValue(const String& varName) {
    try {
        const auto e = ParameterRegistry::getInstance().getParameterById(varName.c_str());

        if (e == nullptr) {
            return "(unknown variable " + varName + ")";
        }

        return e->getFormattedValue();
    } catch (const std::exception& e) {
        LOGF(ERROR, "getValue failed for %s: %s", varName.c_str(), e.what());
        return "(error retrieving " + varName + ")";
    }
}

inline void paramToJson(const String& name, const std::shared_ptr<Parameter>& param, JsonVariant doc) {
    try {
        doc["type"] = param->getType();
        doc["name"] = name;
        doc["displayName"] = param->getDisplayName();
        doc["section"] = param->getSection();
        doc["position"] = param->getPosition();
        doc["hasHelpText"] = param->hasHelpText();
        doc["show"] = param->shouldShow();

        // Set parameter value using the appropriate method based on type
        switch (param->getType()) {
            case kInteger:
                doc["value"] = static_cast<int>(param->getValue());
                break;

            case kUInt8:
                doc["value"] = static_cast<uint8_t>(param->getValue());
                break;

            case kDouble:
                doc["value"] = round2(param->getValue());
                break;

            case kFloat:
                doc["value"] = round2(static_cast<float>(param->getValue()));
                break;

            case kCString:
                doc["value"] = param->getStringValue();
                break;

            case kEnum:
                {
                    doc["value"] = static_cast<int>(param->getValue());

                    const JsonArray options = doc["options"].to<JsonArray>();
                    const char* const* enumOptions = param->getEnumOptions();
                    const size_t enumCount = param->getEnumCount();

                    for (size_t i = 0; i < enumCount && enumOptions[i] != nullptr; i++) {
                        auto optionObj = options.add<JsonObject>();
                        optionObj["value"] = static_cast<int>(i);
                        optionObj["label"] = enumOptions[i];
                    }

                    break;
                }

            default:
                doc["value"] = param->getValue();
                break;
        }

        doc["min"] = param->getMinValue();
        doc["max"] = param->getMaxValue();
    } catch (const std::exception& e) {
        LOGF(ERROR, "paramToJson failed for %s: %s", name.c_str(), e.what());
    }
}

inline String getHeader(const String& varName) {
    static const std::unordered_map<std::string, const char*> headers = {
        {"FONTAWESOME", R"(<link href="/css/fontawesome-6.2.1.min.css" rel="stylesheet">)"},
        {"BOOTSTRAP", R"(<link href="/css/bootstrap-5.2.3.min.css" rel="stylesheet">)"},
        {"BOOTSTRAP_BUNDLE", "<script src=\"/js/bootstrap.bundle.5.2.3.min.js\"></script>"},
        {"VUEJS", "<script src=\"/js/vue.3.2.47.min.js\"></script>"},
        {"VUE_NUMBER_INPUT", "<script src=\"/js/vue-number-input.min.js\"></script>"},
        {"UPLOT", R"(<script src="/js/uPlot.1.6.28.min.js"></script><link rel="stylesheet" href="/css/uPlot.min.css">)"}
    };

    const auto it = headers.find(varName.c_str());
    return it != headers.end() ? String(it->second) : String("");
}

inline String staticProcessor(const String& var) {
    try {
        // try replacing var for variables in ParameterRegistry
        if (var.startsWith("VAR_SHOW_")) {
            return getValue(var.substring(9)); // cut off "VAR_SHOW_"
        }

        if (var.startsWith("VAR_HEADER_")) {
            return getHeader(var.substring(11)); // cut off "VAR_HEADER_"
        }

        // var didn't start with above names, try opening var as fragment file and use contents if it exists
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
            LOGF(DEBUG, "Can't open file %s, not enough memory available", fragmentPath.c_str());
        }
        else {
            LOGF(DEBUG, "Fragment %s not found", fragmentPath.c_str());
        }

        // didn't find a value for the var, replace var with empty string
        return String();
    } catch (const std::exception& e) {
        LOGF(ERROR, "staticProcessor failed for %s: %s", var.c_str(), e.what());
        return String();
    }
}

// ==================== OPTIMIZED API HANDLERS ====================

enum class ToggleType { STEAM, PID, BACKFLUSH, SCALE_TARE, SCALE_CALIBRATION };

void handleToggle(AsyncWebServerRequest* request, ToggleType type) {
    try {
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

        bool* targetVar = nullptr;
        const char* logName = nullptr;
        const char* jsonKey = nullptr;

        switch (type) {
            case ToggleType::STEAM:
                {
                    const bool steamMode = !steamON;
                    setSteamMode(steamMode);
                    LOGF(DEBUG, "Toggle steam mode: %s", steamON ? "on" : "off");
                    request->send(200, "application/json", JsonResponseBuilder::createBoolResponse("steamMode", steamON));
                    return;
                }
            case ToggleType::BACKFLUSH:
                targetVar = &backflushOn;
                logName = "backflush mode";
                jsonKey = "backflushOn";
                backflushOn = !backflushOn;
                break;
            case ToggleType::SCALE_TARE:
                targetVar = &scaleTareOn;
                logName = "scale tare mode";
                jsonKey = "scaleTareOn";
                scaleTareOn = !scaleTareOn;
                break;
            case ToggleType::SCALE_CALIBRATION:
                targetVar = &scaleCalibrationOn;
                logName = "scale calibration mode";
                jsonKey = "scaleCalibrationOn";
                scaleCalibrationOn = !scaleCalibrationOn;
                break;
            case ToggleType::PID:
                {
                    LOGF(DEBUG, "/api/pid requested, method: %d", request->method());

                    const auto pidParam = ParameterRegistry::getInstance().getParameterById("pid.enabled");
                    const bool newPidState = !pidParam->getValueAs<bool>();
                    ParameterRegistry::getInstance().setParameterValue("pid.enabled", newPidState);

                    pidON = newPidState;

                    LOGF(DEBUG, "Toggle PID state: %d\n", newPidState);

                    request->send(200, "application/json", JsonResponseBuilder::createBoolResponse("pidEnabled", newPidState));
                    return;
                }
        }

        LOGF(DEBUG, "Toggle %s: %s", logName, *targetVar ? "on" : "off");
        request->send(200, "application/json", JsonResponseBuilder::createBoolResponse(jsonKey, *targetVar));
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleToggle failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Internal server error"));
    }
}

// Individual handler wrappers
inline void handleToggleSteam(AsyncWebServerRequest* request) {
    handleToggle(request, ToggleType::STEAM);
}

inline void handleTogglePid(AsyncWebServerRequest* request) {
    handleToggle(request, ToggleType::PID);
}

inline void handleToggleBackflush(AsyncWebServerRequest* request) {
    handleToggle(request, ToggleType::BACKFLUSH);
}

inline void handleToggleTareScale(AsyncWebServerRequest* request) {
    handleToggle(request, ToggleType::SCALE_TARE);
}

inline void handleToggleScaleCalibration(AsyncWebServerRequest* request) {
    handleToggle(request, ToggleType::SCALE_CALIBRATION);
}

// Handler for last measured weight
inline void handleWeight(AsyncWebServerRequest* request) {
    try {
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }
        String json = getWeightJsonString();
        request->send(200, "application/json", json);
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleWeight failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Weight data unavailable"));
    }
}

inline void handleParameters(AsyncWebServerRequest* request) {
    try {
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

        logMemoryUsage("handleParameters start");

        if (request->method() == 1) { // HTTP_GET
            const auto& registry = ParameterRegistry::getInstance();
            const auto& parameters = registry.getParameters();

            // Check for filter parameter
            String filterType = "";
            if (request->hasParam("filter")) {
                filterType = request->getParam("filter")->value();
            }

            AsyncJsonResponse* response = new AsyncJsonResponse(false);
            JsonArray array = response->getRoot().to<JsonArray>();

            int filteredParameterCount = 0;

            // Get parameters based on filter
            for (const auto& param : parameters) {
                if (!param->shouldShow()) continue;

                bool includeParam = false;

                if (filterType == "hardware") {
                    includeParam = param->getSection() >= 11 && param->getSection() <= 15;
                }
                else if (filterType == "behavior") {
                    includeParam = param->getSection() >= 0 && param->getSection() <= 9;
                }
                else if (filterType == "other") {
                    includeParam = param->getSection() == 10;
                }
                else if (filterType == "all") {
                    includeParam = true;
                }
                else {
                    includeParam = param->getSection() == 0 || param->getSection() == 1 || param->getSection() == 10;
                }

                if (includeParam) {
                    JsonObject paramObj = array.add<JsonObject>();
                    paramToJson(param->getId(), param, paramObj);
                    filteredParameterCount++;
                }
            }

            LOGF(DEBUG, "/parameters returning %d parameters", filteredParameterCount);
            response->setLength();
            request->send(response);
        }
        else if (request->method() == 2) { // HTTP_POST
            auto& registry = ParameterRegistry::getInstance();

            String responseMessage = "OK";
            bool hasErrors = false;

            const auto requestParams = request->params();

            for (auto i = 0u; i < requestParams; ++i) {
                if (auto* p = request->getParam(i); p && p->name().length() > 0 && p->value().length() > 0) {
                    const String& varName = p->name();
                    const String& value = p->value();

                    try {
                        std::shared_ptr<Parameter> paramPtr = registry.getParameterById(varName.c_str());

                        if (paramPtr == nullptr || !paramPtr->shouldShow()) {
                            continue;
                        }

                        if (paramPtr->getType() == kCString) {
                            registry.setParameterValue(varName.c_str(), value);
                        }
                        else {
                            if (!isValidNumber(value)) {
                                LOGF(WARNING, "Invalid number format for parameter %s: %s", varName.c_str(), value.c_str());
                                hasErrors = true;
                                continue;
                            }
                            double newVal = std::stod(value.c_str());
                            registry.setParameterValue(varName.c_str(), newVal);
                        }
                    } catch (const std::exception& e) {
                        LOGF(ERROR, "Parameter %s processing failed: %s", varName.c_str(), e.what());
                        hasErrors = true;
                        continue; // Skip this parameter instead of crashing
                    }
                }
            }

            registry.forceSave();
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
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

        JsonDocument doc;
        auto* p = request->getParam("param");

        if (p == nullptr) {
            request->send(422, "application/json", JsonResponseBuilder::createErrorResponse("parameter is missing"));
            return;
        }

        const String& varValue = p->value();

        const std::shared_ptr<Parameter> param = ParameterRegistry::getInstance().getParameterById(varValue.c_str());

        if (param == nullptr) {
            request->send(404, "application/json", JsonResponseBuilder::createErrorResponse("parameter not found"));
            return;
        }

        doc["name"] = varValue;
        doc["helpText"] = param->getHelpText();

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
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

        const String json = getTempString();
        request->send(200, "application/json", json);
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleTemperatures failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Temperature data unavailable"));
    }
}

inline void handleTimeseries(AsyncWebServerRequest* request) {
    try {
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

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
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

        request->send(200, "application/json", JsonResponseBuilder::createSuccessResponse("WiFi settings are being reset. Rebooting..."));

        // Defer slightly so the response gets sent before reboot
        delay(1000);

        wiFiReset();
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleWifiReset failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("WiFi reset failed"));
    }
}

inline void handleConfigDownload(AsyncWebServerRequest* request) {
    try {
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

        if (!LittleFS.exists("/config.json")) {
            request->send(404, "application/json", JsonResponseBuilder::createErrorResponse("Config file not found"));
            return;
        }

        File configFile = LittleFS.open("/config.json", "r");

        if (!configFile) {
            request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Failed to open config file"));
            return;
        }

        JsonDocument doc;
        const DeserializationError error = deserializeJson(doc, configFile);
        configFile.close();

        if (error) {
            request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Failed to parse config file"));
            return;
        }

        // Serialize as pretty JSON
        String prettifiedJson;
        serializeJsonPretty(doc, prettifiedJson);

        // Send the prettified JSON
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
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

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

            if (bool isValid = config.validateAndApplyFromJson(uploadBuffer)) {
                LOG(INFO, "Configuration validated and applied successfully");

                AsyncWebServerResponse* response = request->beginResponse(200, "application/json",
                    R"({"success": true, "message": "Configuration validated and applied successfully.", "restart": true})");

                response->addHeader("Connection", "close");
                request->send(response);
            }
            else {
                LOG(ERROR, "Configuration validation failed - invalid data or out of range values");

                AsyncWebServerResponse* response = request->beginResponse(400, "application/json",
                    R"({"success": false, "message": "Configuration validation failed. Please check that all parameter values are within valid ranges.", "restart": true})");

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
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

        request->send(200, "application/json", JsonResponseBuilder::createSuccessResponse("Restarting..."));
        delay(100);
        ESP.restart();
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleRestart failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Restart failed"));
    }
}

inline void handleFactoryReset(AsyncWebServerRequest* request) {
    try {
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

        const bool removed = LittleFS.remove("/config.json");

        request->send(200, "application/json",
            removed ? JsonResponseBuilder::createSuccessResponse("Factory reset. Restarting...")
                    : JsonResponseBuilder::createErrorResponse("Could not delete config.json. Restarting..."));

        delay(100);
        ESP.restart();
    } catch (const std::exception& e) {
        LOGF(ERROR, "handleFactoryReset failed: %s", e.what());
        request->send(500, "application/json", JsonResponseBuilder::createErrorResponse("Factory reset failed"));
    }
}

// Setup API routes with /api/ prefix
inline void setupApiRoutes() {
    // Health check
    server.on("/api/health", HTTP_GET, [](AsyncWebServerRequest* request) { request->send(200); });

    // Machine control endpoints
    server.on("/api/steam", HTTP_POST, handleToggleSteam);
    server.on("/api/pid", HTTP_POST, handleTogglePid);
    server.on("/api/backflush", HTTP_POST, handleToggleBackflush);

    // Scale endpoints (conditional)
    if (config.get<bool>("hardware.sensors.scale.enabled")) {
        server.on("/api/scale/tare", HTTP_POST, handleToggleTareScale);
        server.on("/api/scale/calibration", HTTP_POST, handleToggleScaleCalibration);
        server.on("/api/scale/weight", HTTP_GET, handleWeight);
    }

    // Data endpoints
    server.on("/api/parameters", HTTP_ANY, handleParameters);
    server.on("/api/parameter-help", HTTP_GET, handleParameterHelp);
    server.on("/api/temperatures", HTTP_GET, handleTemperatures);
    server.on("/api/history", HTTP_GET, handleTimeseries);

    // System endpoints
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
}

// Enhanced 404 handler that serves SPA for UI routes and proper 404 for API routes
inline void handleNotFound(AsyncWebServerRequest* request) {
    String path = request->url();

    // API routes should return proper JSON 404
    if (path.startsWith("/api/")) {
        request->send(404, "application/json", JsonResponseBuilder::createErrorResponse("API endpoint not found"));
        return;
    }

    // For UI routes without extensions (SPA routing), serve index.html
    if (path.startsWith("/ui/") && path.indexOf('.') == -1) {
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

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

    // For any other path, return proper 404
    request->send(404, "text/plain", "Not found");
}

// ==================== FIXED SERVER SETUP ====================

inline void serverSetup() {
    logMemoryUsage("serverSetup start");

    // Setup API routes first
    setupApiRoutes();

    // Root redirect
    server.on("/", HTTP_GET, [](AsyncWebServerRequest* request) {
        request->redirect("/ui/");
    });

    // Fallback handler for SPA client-side routing (only for routes without extensions)
    server.on("/ui", HTTP_GET, [](AsyncWebServerRequest* request) {
        if (!authenticate(request)) {
            return request->requestAuthentication();
        }

        String path = request->url();
        LOGF(DEBUG, "UI wildcard request for: %s", path.c_str());

        // Only serve index.html for routes without file extensions (SPA routing)
        if (path.indexOf('.') == -1) {
            LOGF(DEBUG, "Serving index.html for SPA route: %s", path.c_str());

            if (serveGzippedFile(request, "/ui/index.html")) {
                return;
            }

            if (LittleFS.exists("/ui/index.html")) {
                request->send(LittleFS, "/ui/index.html", "text/html");
                return;
            }
        }


        if (serveGzippedFile(request, path)) {
            return;
        }

        // For files with extensions, this shouldn't be reached due to serveStatic above
        request->send(404, "text/plain", "File not found");
    });

    // Enhanced 404 handler for both API and UI routes
    server.onNotFound(handleNotFound);

    // set up event handler for temperature messages
    events.onConnect([](AsyncEventSourceClient* client) {
        if (client->lastId()) {
            LOGF(DEBUG, "Reconnected, last message ID was: %u", client->lastId());
        }

        client->send("hello", nullptr, millis(), 10000);
    });

    server.addHandler(&events);

    // serve static files
    LittleFS.begin();

    server.addMiddleware(&corsMiddleware);
    server.begin();

    LOG(INFO, ("Server started at " + WiFi.localIP().toString()).c_str());
    logMemoryUsage("serverSetup complete");
}

inline void sendTempEvent(const double currentTemp, const double targetTemp, const double heaterPower) {
    try {
        // save all values in memory to show history using optimized class
        tempHistory.addPoint(currentTemp, targetTemp, heaterPower);

        events.send("ping", nullptr, millis());
        events.send(getTempString().c_str(), "new_temps", millis());
    } catch (const std::exception& e) {
        LOGF(ERROR, "sendTempEvent failed: %s", e.what());
    }
}

inline void sendWeightEvent() {
    try {
        // Send weight event
        String weightJson = getWeightJsonString();
        events.send(weightJson.c_str(), "weight", millis());
    } catch (const std::exception& e) {
        LOGF(ERROR, "sendWeightEvent failed: %s", e.what());
    }
}
