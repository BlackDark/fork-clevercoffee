/**
 * @file embeddedWebserver.h
 *
 * @brief Embedded webserver
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

// why do we need these?
inline double curTemp = 0.0;
inline double tTemp = 0.0;
inline double hPower = 0.0;

extern float currReadingWeight; // defined in brewHandler.h
extern float currBrewWeight; // defined in brewHandler.h
extern double temperature; // defined in main.cpp
extern double brewSetpoint; // defined in main.cpp
extern double pidOutput; // defined in main.cpp, needs to be divided by 10 for display

#define HISTORY_LENGTH 600 // 30 mins of values (20 vals/min * 60 min) = 600 (7,2kb)

static float tempHistory[3][HISTORY_LENGTH] = {};
inline int historyCurrentIndex = 0;
inline int historyValueCount = 0;

void serverSetup();

inline bool authenticate(AsyncWebServerRequest* request) {
    if (!config.get<bool>("system.auth.enabled")) {
        return true;
    }

    const auto clientIP = request->client()->remoteIP().toString();
    const auto requestedPath = request->url();
    const auto username = config.get<String>("system.auth.username");
    const auto password = config.get<String>("system.auth.password");

    if (request->authenticate(username.c_str(), password.c_str())) {
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

inline uint8_t flipUintValue(const uint8_t value) {
    return (value + 3) % 2;
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

inline String getTempString() {
    JsonDocument doc;

    doc["currentTemp"] = temperature;
    doc["targetTemp"] = brewSetpoint;
    doc["heaterPower"] = pidOutput / 10;

    String json;
    serializeJson(doc, json);

    return json;
}

inline String getWeightJsonString() {
    extern float currReadingWeight;

    JsonDocument doc;

    doc["weight"] = round2(currReadingWeight);
    doc["brewWeight"] = round2(currBrewWeight);

    String json;
    serializeJson(doc, json);

    return json;
}

inline String getValue(const String& varName) {
    try {
        const auto e = ParameterRegistry::getInstance().getParameterById(varName.c_str());

        if (e == nullptr) {
            return "(unknown variable " + varName + ")";
        }

        return e->getFormattedValue();
    } catch (const std::out_of_range&) {
        return "(unknown variable " + varName + ")";
    }
}

inline void paramToJson(const String& name, const std::shared_ptr<Parameter>& param, JsonVariant doc) {
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
}

inline String getHeader(const String& varName) {
    static const std::unordered_map<std::string, const char*> headers = {
        {"FONTAWESOME", R"(<link href="/css/fontawesome-6.2.1.min.css" rel="stylesheet">)"}, {"BOOTSTRAP", R"(<link href="/css/bootstrap-5.2.3.min.css" rel="stylesheet">)"},
        {"BOOTSTRAP_BUNDLE", "<script src=\"/js/bootstrap.bundle.5.2.3.min.js\"></script>"}, {"VUEJS", "<script src=\"/js/vue.3.2.47.min.js\"></script>"},
        {"VUE_NUMBER_INPUT", "<script src=\"/js/vue-number-input.min.js\"></script>"},       {"UPLOT", R"(<script src="/js/uPlot.1.6.28.min.js"></script><link rel="stylesheet" href="/css/uPlot.min.css">)"}};

    const auto it = headers.find(varName.c_str());
    return it != headers.end() ? String(it->second) : String("");
}

inline String staticProcessor(const String& var) {
    // try replacing var for variables in ParameterRegistry
    if (var.startsWith("VAR_SHOW_")) {
        return getValue(var.substring(9)); // cut off "VAR_SHOW_"
    }

    if (var.startsWith("VAR_HEADER_")) {
        return getHeader(var.substring(11)); // cut off "VAR_HEADER_"
    }

    // var didn't start with above names, try opening var as fragment file and use contents if it exists
    // TODO: this seems to consume too much heap in some cases, probably better to remove fragment loading and only use one SPA in the long term (or only support ESP32 which has more RAM)
    String varLower(var);
    varLower.toLowerCase();

    if (File file = LittleFS.open("/html_fragments/" + varLower + ".html", "r")) {
        if (file.size() * 2 < ESP.getFreeHeap()) {
            String ret = file.readString();
            file.close();
            return ret;
        }

        LOGF(DEBUG, "Can't open file %s, not enough memory available", file.name());
    }
    else {
        LOGF(DEBUG, "Fragment %s not found", varLower.c_str());
    }

    // didn't find a value for the var, replace var with empty string
    return {};
}

// API Handler Functions
inline void handleToggleSteam(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    const bool steamMode = !steamON;
    setSteamMode(steamMode);

    LOGF(DEBUG, "Toggle steam mode: %s", steamON ? "on" : "off");

    request->send(200, "application/json", "{\"success\": true, \"steamMode\": " + String(steamON ? "true" : "false") + "}");
}

inline void handleTogglePid(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    LOGF(DEBUG, "/api/pid requested, method: %d", request->method());

    const auto pidParam = ParameterRegistry::getInstance().getParameterById("pid.enabled");
    const bool newPidState = !pidParam->getValueAs<bool>();
    ParameterRegistry::getInstance().setParameterValue("pid.enabled", newPidState);

    pidON = newPidState;

    LOGF(DEBUG, "Toggle PID state: %d\n", newPidState);

    request->send(200, "application/json", "{\"success\": true, \"pidEnabled\": " + String(newPidState ? "true" : "false") + "}");
}

inline void handleToggleBackflush(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    backflushOn = !backflushOn;
    LOGF(DEBUG, "Toggle backflush mode: %s", backflushOn ? "on" : "off");

    request->send(200, "application/json", "{\"success\": true, \"backflushOn\": " + String(backflushOn ? "true" : "false") + "}");
}

inline void handleToggleTareScale(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    scaleTareOn = !scaleTareOn;

    LOGF(DEBUG, "Toggle scale tare mode: %s", scaleTareOn ? "on" : "off");

    request->send(200, "application/json", "{\"success\": true, \"scaleTareOn\": " + String(scaleTareOn ? "true" : "false") + "}");
}

inline void handleToggleScaleCalibration(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    scaleCalibrationOn = !scaleCalibrationOn;

    LOGF(DEBUG, "Toggle scale calibration mode: %s", scaleCalibrationOn ? "on" : "off");

    request->send(200, "application/json", "{\"success\": true, \"scaleCalibrationOn\": " + String(scaleCalibrationOn ? "true" : "false") + "}");
}

// Handler for last measured weight
inline void handleWeight(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }
    String json = getWeightJsonString();
    request->send(200, "application/json", json);
}

inline void handleParameters(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

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
                        double newVal = std::stod(value.c_str());
                        registry.setParameterValue(varName.c_str(), newVal);
                    }
                } catch (const std::exception& e) {
                    LOGF(INFO, "Parameter %s processing failed: %s", varName.c_str(), e.what());
                    hasErrors = true;
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
}

inline void handleParameterHelp(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    JsonDocument doc;
    auto* p = request->getParam("param");

    if (p == nullptr) {
        request->send(422, "application/json", "{\"error\": \"parameter is missing\"}");
        return;
    }

    const String& varValue = p->value();

    const std::shared_ptr<Parameter> param = ParameterRegistry::getInstance().getParameterById(varValue.c_str());

    if (param == nullptr) {
        request->send(404, "application/json", "{\"error\": \"parameter not found\"}");
        return;
    }

    doc["name"] = varValue;
    doc["helpText"] = param->getHelpText();

    String helpJson;
    serializeJson(doc, helpJson);
    request->send(200, "application/json", helpJson);
}

inline void handleTemperatures(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    const String json = getTempString();
    request->send(200, "application/json", json);
}

// TODO: could send values also chunked and without json (but needs three
// endpoints then?)
// https://stackoverflow.com/questions/61559745/espasyncwebserver-serve-large-array-from-ram

inline void handleTimeseries(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    AsyncResponseStream* response = request->beginResponseStream("application/json");
    response->addHeader("Connection", "close"); // Force connection close

    JsonDocument doc;

    // for each value in mem history array, add json array element
    auto currentTemps = doc["currentTemps"].to<JsonArray>();
    auto targetTemps = doc["targetTemps"].to<JsonArray>();
    auto heaterPowers = doc["heaterPowers"].to<JsonArray>();

    // go through history values backwards starting from currentIndex and
    // wrap around beginning to include valueCount many values
    for (int i = mod(historyCurrentIndex - historyValueCount, HISTORY_LENGTH); i != mod(historyCurrentIndex, HISTORY_LENGTH); i = mod(i + 1, HISTORY_LENGTH)) {
        currentTemps.add(round2(tempHistory[0][i]));
        targetTemps.add(round2(tempHistory[1][i]));
        heaterPowers.add(round2(tempHistory[2][i]));
    }

    if (doc.overflowed()) {
        request->send(500, "application/json", "{\"error\": \"timeseries JSON overflowed\"}");
        return;
    }

    String out;
    out.reserve(measureJson(doc) + 16);
    serializeJson(doc, out);
    request->send(200, "application/json", out);
}

inline void handleWifiReset(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    request->send(200, "application/json", "{\"success\": true, \"message\": \"WiFi settings are being reset. Rebooting...\"}");

    // Defer slightly so the response gets sent before reboot
    delay(1000);

    wiFiReset();
}

inline void handleConfigDownload(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    if (!LittleFS.exists("/config.json")) {
        request->send(404, "application/json", "{\"error\": \"Config file not found\"}");
        return;
    }

    File configFile = LittleFS.open("/config.json", "r");

    if (!configFile) {
        request->send(500, "application/json", "{\"error\": \"Failed to open config file\"}");
        return;
    }

    JsonDocument doc;
    const DeserializationError error = deserializeJson(doc, configFile);
    configFile.close();

    if (error) {
        request->send(500, "application/json", "{\"error\": \"Failed to parse config file\"}");
        return;
    }

    // Serialize as pretty JSON
    String prettifiedJson;
    serializeJsonPretty(doc, prettifiedJson);

    // Send the prettified JSON
    AsyncWebServerResponse* response = request->beginResponse(200, "application/json", prettifiedJson);
    response->addHeader("Content-Disposition", "attachment; filename=\"config.json\"");
    request->send(response);
}

inline void handleConfigUpload(AsyncWebServerRequest* request, const String& filename, const size_t index, const uint8_t* data, const size_t len, const bool final) {
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
}

inline void handleRestart(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    request->send(200, "application/json", "{\"success\": true, \"message\": \"Restarting...\"}");
    delay(100);
    ESP.restart();
}

inline void handleFactoryReset(AsyncWebServerRequest* request) {
    if (!authenticate(request)) {
        return request->requestAuthentication();
    }

    const bool removed = LittleFS.remove("/config.json");

    request->send(200, "application/json", removed ? "{\"success\": true, \"message\": \"Factory reset. Restarting...\"}" : "{\"success\": false, \"message\": \"Could not delete config.json. Restarting...\"}");

    delay(100);
    ESP.restart();
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
        request->send(404, "application/json", "{\"error\": \"API endpoint not found\"}");
        return;
    }

    // All UI routes under /ui/ should serve the SPA and let React handle routing
    if (path.startsWith("/ui/")) {
        if (File file = LittleFS.open("/ui/index.html", "r")) {
            request->send(LittleFS, "/ui/index.html", "text/html");
            return;
        }
        else {
            request->send(404, "text/plain", "UI files not found");
            return;
        }
    }

    // For any other path, return proper 404
    request->send(404, "text/plain", "Not found");
}

inline void serverSetup() {
    // Setup API routes
    setupApiRoutes();

    // Add redirect from root to /ui/
    server.on("/", HTTP_GET, [](AsyncWebServerRequest* request) {
        // Check if this is exactly the root path (no query params for UI routes)
        String path = request->url();
        if (path == "/") {
            request->redirect("/ui/");
            return;
        }

        // If it's a root path with parameters, serve the SPA
        if (File file = LittleFS.open("/ui/index.html", "r")) {
            request->send(LittleFS, "/ui/index.html", "text/html");
        }
        else {
            request->send(404, "text/plain", "UI not found");
        }
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

    // Serve UI at /ui/ path only (root is handled by redirect above)
    server.serveStatic("/ui", LittleFS, "/ui/", "max-age=604800").setDefaultFile("index.html");

    server.addMiddleware(&corsMiddleware);
    server.begin();

    LOG(INFO, ("Server started at " + WiFi.localIP().toString()).c_str());
}

// skip counter so we don't keep a value every second
inline int skippedValues = 0;
#define SECONDS_TO_SKIP 2

inline void sendTempEvent(const double currentTemp, const double targetTemp, const double heaterPower) {
    curTemp = currentTemp;
    tTemp = targetTemp;
    hPower = heaterPower;

    // save all values in memory to show history
    if (skippedValues > 0 && skippedValues % SECONDS_TO_SKIP == 0) {
        // use array and int value for start index (round robin)
        // one record (3 float values == 12 bytes) every three seconds, for half
        // an hour -> 7.2kB of static memory
        tempHistory[0][historyCurrentIndex] = static_cast<float>(currentTemp);
        tempHistory[1][historyCurrentIndex] = static_cast<float>(targetTemp);
        tempHistory[2][historyCurrentIndex] = static_cast<float>(heaterPower);
        historyCurrentIndex = (historyCurrentIndex + 1) % HISTORY_LENGTH;
        historyValueCount = min(HISTORY_LENGTH - 1, historyValueCount + 1);
        skippedValues = 0;
    }
    else {
        skippedValues++;
    }

    events.send("ping", nullptr, millis());
    events.send(getTempString().c_str(), "new_temps", millis());
}

inline void sendWeightEvent() {
    // Send weight event
    String weightJson = getWeightJsonString();
    events.send(weightJson.c_str(), "weight", millis());
}

