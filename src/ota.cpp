/**
 * @file ota.cpp
 * @brief Over-the-Air (OTA) firmware update implementation
 */

#include "clevercoffee/ota.h"

#include "clevercoffee/Logger.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/display/DisplayOtaScreen.h"
#include "clevercoffee/utils/Resilience.h"
#include "clevercoffee/utils/helperUtils.h"

#include <Arduino.h>
#include <ArduinoJson.h>
#include <ArduinoOTA.h>

namespace OTA {

namespace {

inline auto& getOTAState() noexcept {
    return OTAStateManager::getInstance();
}

constexpr unsigned long OTA_ERROR_DISPLAY_MS    = 30000;
constexpr unsigned long OTA_COMPLETE_DISPLAY_MS = 5000;
constexpr unsigned long OTA_RESTART_DELAY_MS    = 1000;

static const char* FILESYSTEM_PARTITION_LABEL = "spiffs";

Watchdog*                    g_watchdog       = nullptr;
CleverCoffee::SystemContext* g_displayContext = nullptr;
OtaSessionCallbacks          g_sessionCallbacks{};
bool                         g_restartPending    = false;
unsigned long                g_restartAtMs       = 0;
uint8_t                      g_lastLoggedDecile  = 255;
uint8_t                      g_lastDisplayDecile = 255;

const char* arduinoOtaErrorMessage(unsigned errorCode) {
    switch (errorCode) {
        case OTA_AUTH_ERROR:
            return "Auth failed";
        case OTA_BEGIN_ERROR:
            return "Begin failed";
        case OTA_CONNECT_ERROR:
            return "Connect failed";
        case OTA_RECEIVE_ERROR:
            return "Receive failed";
        case OTA_END_ERROR:
            return "End failed";
        default:
            return "Update failed";
    }
}

void invokePrepareHardware() {
    if (g_sessionCallbacks.prepareHardware) {
        g_sessionCallbacks.prepareHardware();
    }
    if (g_watchdog) {
        g_watchdog->suspend();
    }
}

void invokeRestoreHardware() {
    if (g_sessionCallbacks.restoreHardware) {
        g_sessionCallbacks.restoreHardware();
    }
    if (g_watchdog) {
        g_watchdog->resume();
    }
}

void refreshDisplayIfNeeded(uint8_t percent) {
    const uint8_t decile = static_cast<uint8_t>(percent / 10U);
    if (decile == g_lastDisplayDecile && percent < 100U) {
        return;
    }
    g_lastDisplayDecile = decile;
    if (g_displayContext) {
        refreshDisplay(*g_displayContext);
    }
}

struct HTTPUpdateResult {
    bool        success       = false;
    int         contentLength = 0;
    WiFiClient* stream        = nullptr;
    HTTPClient* http          = nullptr;
};

HTTPUpdateResult initHttpUpdate(const String& url, HTTPClient& http, WiFiClient& client) {
    HTTPUpdateResult result;

    http.begin(client, url);
    http.addHeader("User-Agent", "ESP32-CleverCoffee-OTA");

    const int httpCode = http.GET();
    if (httpCode != HTTP_CODE_OK) {
        LOGF(ERROR, "HTTP GET failed, error: %d", httpCode);
        getOTAState().setUpdateError(true, "HTTP GET failed, error: " + String(httpCode));
        http.end();
        return result;
    }

    result.contentLength = http.getSize();
    if (result.contentLength <= 0) {
        LOG(ERROR, "Content-Length not found or zero");
        getOTAState().setUpdateError(true, "Content-Length not found or zero");
        http.end();
        return result;
    }

    result.stream  = http.getStreamPtr();
    result.http    = &http;
    result.success = true;
    return result;
}

bool downloadAndWrite(WiFiClient* stream, HTTPClient& http, int contentLength) {
    size_t  written = 0;
    uint8_t buffer[OTA_BUFFER_SIZE];

    while (http.connected() && (written < static_cast<size_t>(contentLength))) {
        const size_t available = stream->available();
        if (available > 0) {
            const size_t toRead    = min(available, sizeof(buffer));
            const size_t bytesRead = stream->readBytes(buffer, toRead);

            if (Update.write(buffer, bytesRead) != bytesRead) {
                LOGF(ERROR, "Write failed at byte %d", written);
                getOTAState().setUpdateError(true, "Write failed at byte " + String(written));
                Update.abort();
                return false;
            }

            written += bytesRead;
            reportProgress(static_cast<uint8_t>((written * 100U) / static_cast<size_t>(contentLength)));

            if (written % static_cast<size_t>(contentLength / 10) == 0) {
                LOGF(INFO, "OTA Progress: %d%%", getOTAState().getCurrentProgress());
            }
        }
        delay(1);
    }

    if (written != static_cast<size_t>(contentLength)) {
        LOGF(ERROR, "Written %d bytes, expected %d", written, contentLength);
        getOTAState().setUpdateError(
            true, "Incomplete download: written " + String(written) + " bytes, expected " + String(contentLength));
        Update.abort();
        return false;
    }

    return true;
}

bool validateFileExtension(const String& filename, bool isFilesystem) {
    String path = filename;
    path.toLowerCase();

    if (isFilesystem) {
        return path.endsWith(".bin") || path.endsWith(".img");
    }
    return path.endsWith(".bin");
}

bool processUploadChunk(AsyncWebServerRequest* request, size_t index, uint8_t* data, size_t len, bool isFilesystem) {
    if (!getOTAState().isUpdateStarted()) {
        return true;
    }

    if (Update.write(data, len) != len) {
        const String errorType = isFilesystem ? Type::Filesystem : Type::Firmware;
        LOGF(ERROR, "OTA %s update write failed at byte %d", errorType, index + len);
        endSessionError("OTA " + errorType + " update write failed at byte " + String(index + len));
        Update.abort();
        request->send(500, "application/json", R"({"success": false, "message": "Write failed"})");
        return false;
    }

    getOTAState().addUploadedSize(len);
    getOTAState().setTotalSize(index + len);

    if (getOTAState().getUploadedSize() > 0) {
        const size_t  minSize  = isFilesystem ? (256 * 1024) : (512 * 1024);
        const uint8_t progress = static_cast<uint8_t>(min(
            90,
            static_cast<int>((getOTAState().getUploadedSize() * 90) / max(getOTAState().getUploadedSize(), minSize))));
        reportProgress(progress);
    }

    return true;
}

void resetStatus() {
    getOTAState().resetState();
    g_lastLoggedDecile  = 255;
    g_lastDisplayDecile = 255;
}

} // namespace

void setWatchdog(Watchdog* watchdog) noexcept {
    g_watchdog = watchdog;
}

void setDisplayContext(CleverCoffee::SystemContext* context) noexcept {
    g_displayContext = context;
}

void setSessionCallbacks(OtaSessionCallbacks callbacks) noexcept {
    g_sessionCallbacks = callbacks;
}

void beginSession(const char* updateType) noexcept {
    resetStatus();
    getOTAState().setUpdateType(updateType);
    getOTAState().setUpdateStatus(Status::Uploading);
    getOTAState().setUpdateStarted(true);
    getOTAState().setCurrentProgress(0);
    getOTAState().setUpdateError(false, "");
    invokePrepareHardware();
    if (g_displayContext) {
        refreshDisplay(*g_displayContext);
    }
}

void endSessionSuccess() noexcept {
    getOTAState().setCurrentProgress(100);
    getOTAState().setUpdateStatus(Status::Complete);
    getOTAState().setUpdateStarted(false);
    getOTAState().setUpdateError(false, "");
    invokeRestoreHardware();
    if (g_displayContext) {
        refreshDisplay(*g_displayContext);
    }
}

void endSessionError(const String& message) noexcept {
    getOTAState().setUpdateStarted(false);
    getOTAState().setUpdateError(true, message);
    invokeRestoreHardware();
    if (g_displayContext) {
        refreshDisplay(*g_displayContext);
    }
}

void reportProgress(uint8_t percent) noexcept {
    getOTAState().setCurrentProgress(percent);
    if (getOTAState().getUpdateStatus() == Status::Idle) {
        getOTAState().setUpdateStatus(Status::Uploading);
    }

    const uint8_t decile = static_cast<uint8_t>(percent / 10U);
    if (decile != g_lastLoggedDecile) {
        LOGF(INFO, "OTA progress: %u%%", percent);
        g_lastLoggedDecile = decile;
    }
    refreshDisplayIfNeeded(percent);
}

bool isActive() noexcept {
    return Update.isRunning() || getOTAState().isUpdateInProgress();
}

void runMainLoopTick() noexcept {
    ArduinoOTA.handle();
    if (g_displayContext) {
        refreshDisplay(*g_displayContext);
    }
    pollPendingRestart();
    yield();
}

void scheduleRestart(unsigned long delayMs) noexcept {
    g_restartPending = true;
    g_restartAtMs    = millis() + delayMs;
}

void pollPendingRestart() noexcept {
    if (!g_restartPending) {
        return;
    }
    if (static_cast<long>(millis() - g_restartAtMs) >= 0) {
        ESP.restart();
    }
}

void refreshDisplay(CleverCoffee::SystemContext& context) noexcept {
    CleverCoffee::Display::refreshOtaDisplay(context);
}

void initializeArduinoOta(const char* hostname, const char* password) {
    ArduinoOTA.setHostname(hostname);
    ArduinoOTA.setPassword(password);

    ArduinoOTA.onStart([]() {
        g_lastLoggedDecile  = 255;
        g_lastDisplayDecile = 255;
        beginSession(Type::Firmware);
        LOG(INFO, "ArduinoOTA update started");
    });

    ArduinoOTA.onProgress([](unsigned int progress, unsigned int total) {
        if (total == 0) {
            return;
        }
        reportProgress(static_cast<uint8_t>((progress * 100U) / total));
    });

    ArduinoOTA.onEnd([]() {
        endSessionSuccess();
        scheduleRestart(OTA_RESTART_DELAY_MS);
        LOG(INFO, "ArduinoOTA update finished");
    });

    ArduinoOTA.onError([](ota_error_t error) {
        endSessionError(arduinoOtaErrorMessage(static_cast<unsigned>(error)));
        LOGF(ERROR, "ArduinoOTA error: %u", static_cast<unsigned>(error));
    });

    ArduinoOTA.begin();
    LOG(INFO, "OTA initialized");
}

bool updateFromURL(const String& url) {
    if (url.isEmpty()) {
        LOG(ERROR, "OTA update URL is empty");
        getOTAState().setUpdateError(true, "OTA update URL is empty");
        return false;
    }

    LOGF(INFO, "Starting OTA update from URL: %s", url.c_str());

    auto& state = getOTAState();
    state.setUpdateError(false);
    state.setCurrentProgress(0);
    state.setUpdateStatus(Status::Downloading);

    WiFiClient client;
    HTTPClient http;

    const HTTPUpdateResult httpResult = initHttpUpdate(url, http, client);
    if (!httpResult.success) {
        return false;
    }

    LOGF(INFO, "Starting OTA update, firmware size: %d bytes", httpResult.contentLength);

    if (!Update.begin(httpResult.contentLength)) {
        LOGF(ERROR, "Cannot begin OTA update: %s", Update.errorString());
        getOTAState().setUpdateError(true, "Cannot begin OTA update: " + String(Update.errorString()));
        http.end();
        return false;
    }

    if (!downloadAndWrite(httpResult.stream, http, httpResult.contentLength)) {
        http.end();
        return false;
    }

    http.end();

    if (Update.end()) {
        if (Update.isFinished()) {
            LOG(INFO, "OTA update completed successfully");
            getOTAState().setCurrentProgress(100);
            getOTAState().setUpdateStatus(Status::Complete);
            return true;
        }
        LOGF(ERROR, "OTA update not finished: %s", Update.errorString());
        getOTAState().setUpdateError(true, "OTA update not finished: " + String(Update.errorString()));
        return false;
    }

    LOGF(ERROR, "OTA update failed: %s", Update.errorString());
    getOTAState().setUpdateError(true, "OTA update failed: " + String(Update.errorString()));
    return false;
}

bool updateFilesystemFromURL(const String& url) {
    if (url.isEmpty()) {
        LOG(ERROR, "OTA filesystem update URL is empty");
        getOTAState().setUpdateError(true, "OTA filesystem update URL is empty");
        return false;
    }

    LOGF(INFO, "Starting OTA filesystem update from URL: %s", url.c_str());

    getOTAState().setUpdateError(false);
    getOTAState().setCurrentProgress(0);
    getOTAState().setUpdateStatus(Status::Downloading);

    if (!validateFileExtension(url, true)) {
        LOGF(ERROR, "Invalid filesystem file extension: %s", url.c_str());
        getOTAState().setUpdateError(true, "Invalid filesystem file extension (.bin or .img required)");
        return false;
    }

    WiFiClient client;
    HTTPClient http;

    const HTTPUpdateResult httpResult = initHttpUpdate(url, http, client);
    if (!httpResult.success) {
        return false;
    }

    LOGF(INFO, "Starting OTA filesystem update, size: %d bytes", httpResult.contentLength);
    if (!Update.begin(UPDATE_SIZE_UNKNOWN, U_SPIFFS, -1, LOW, FILESYSTEM_PARTITION_LABEL)) {
        LOGF(ERROR, "Cannot begin OTA filesystem update: %s", Update.errorString());
        getOTAState().setUpdateError(true, "Cannot begin OTA filesystem update: " + String(Update.errorString()));
        http.end();
        return false;
    }

    if (!downloadAndWrite(httpResult.stream, http, httpResult.contentLength)) {
        http.end();
        return false;
    }

    http.end();

    if (Update.end(true)) {
        if (Update.isFinished()) {
            LOG(INFO, "OTA filesystem update completed successfully");
            getOTAState().setCurrentProgress(100);
            getOTAState().setUpdateStatus(Status::Complete);
            return true;
        }
        LOGF(ERROR, "OTA filesystem update not finished: %s", Update.errorString());
        getOTAState().setUpdateError(true, "OTA filesystem update not finished: " + String(Update.errorString()));
        return false;
    }

    LOGF(ERROR, "OTA filesystem update failed: %s", Update.errorString());
    getOTAState().setUpdateError(true, "OTA filesystem update failed: " + String(Update.errorString()));
    return false;
}

void handleFirmwareUpload(
    AsyncWebServerRequest* request, const String& filename, size_t index, uint8_t* data, size_t len, bool final) {
    LOGF(INFO,
         "handleFirmwareUpload called: filename=%s, index=%d, len=%d, final=%d",
         filename.c_str(),
         static_cast<int>(index),
         static_cast<int>(len),
         final);

    if (index == 0 && !validateFileExtension(filename, false)) {
        LOGF(ERROR, "Invalid firmware file: %s (expected .bin extension)", filename.c_str());
        request->send(400,
                      "application/json",
                      R"({"success": false, "message": "Invalid firmware file. Expected .bin extension."})");
        return;
    }

    if (index == 0) {
        if (isUpdateInProgress()) {
            LOGF(WARNING,
                 "Rejected concurrent OTA file upload attempt (current status: %s, updateStarted: %s, "
                 "Update.isRunning(): %s)",
                 getOTAState().getUpdateStatus().c_str(),
                 getOTAState().isUpdateStarted() ? "true" : "false",
                 Update.isRunning() ? "true" : "false");
            request->send(
                409,
                "application/json",
                R"({"success": false, "message": "OTA update already in progress. Please wait for current update to complete."})");
            return;
        }

        LOGF(INFO, "OTA firmware update started: %s", filename.c_str());

        if (!Update.begin(UPDATE_SIZE_UNKNOWN)) {
            LOGF(ERROR, "OTA update failed to begin: %s", Update.errorString());
            request->send(500, "application/json", R"({"success": false, "message": "Failed to begin update"})");
            return;
        }

        beginSession(Type::Firmware);
    }

    if (!processUploadChunk(request, index, data, len, false)) {
        return;
    }

    if (final) {
        getOTAState().setUpdateStatus(Status::Processing);
        reportProgress(95);

        if (getOTAState().isUpdateStarted() && Update.end(true)) {
            LOGF(INFO, "OTA firmware update completed successfully: %d bytes", getOTAState().getTotalSize());
            endSessionSuccess();
            request->send(
                200, "application/json", R"({"success": true, "message": "Update successful. Device will restart."})");
            scheduleRestart(OTA_RESTART_DELAY_MS);
        } else {
            LOGF(ERROR, "OTA update failed to finalize: %s", Update.errorString());
            endSessionError("OTA update failed to finalize: " + String(Update.errorString()));
            const String errorResponse =
                "{\"success\": false, \"message\": \"" + getOTAState().getErrorMessage() + "\"}";
            request->send(500, "application/json", errorResponse);
        }
    }
}

void handleFilesystemUpload(
    AsyncWebServerRequest* request, const String& filename, size_t index, uint8_t* data, size_t len, bool final) {
    LOGF(INFO,
         "handleFilesystemUpload called: filename=%s, index=%d, len=%d, final=%d",
         filename.c_str(),
         static_cast<int>(index),
         static_cast<int>(len),
         final);

    if (index == 0 && !validateFileExtension(filename, true)) {
        LOGF(ERROR, "Invalid filesystem file: %s (expected .bin or .img extension)", filename.c_str());
        request->send(400,
                      "application/json",
                      R"({"success": false, "message": "Invalid filesystem file. Expected .bin or .img extension."})");
        return;
    }

    if (index == 0) {
        if (isUpdateInProgress()) {
            LOGF(WARNING,
                 "Rejected concurrent OTA filesystem upload attempt (current status: %s, updateStarted: %s, "
                 "Update.isRunning(): %s)",
                 getOTAState().getUpdateStatus().c_str(),
                 getOTAState().isUpdateStarted() ? "true" : "false",
                 Update.isRunning() ? "true" : "false");
            request->send(
                409,
                "application/json",
                R"({"success": false, "message": "OTA update already in progress. Please wait for current update to complete."})");
            return;
        }

        LOGF(INFO, "OTA filesystem update started: %s (partition: %s)", filename.c_str(), FILESYSTEM_PARTITION_LABEL);

        if (!Update.begin(UPDATE_SIZE_UNKNOWN, U_SPIFFS, -1, LOW, FILESYSTEM_PARTITION_LABEL)) {
            LOGF(ERROR,
                 "OTA filesystem update failed to begin on partition '%s': %s",
                 FILESYSTEM_PARTITION_LABEL,
                 Update.errorString());
            request->send(
                500, "application/json", R"({"success": false, "message": "Failed to begin filesystem update"})");
            return;
        }

        beginSession(Type::Filesystem);
    }

    if (!processUploadChunk(request, index, data, len, true)) {
        return;
    }

    if (final) {
        getOTAState().setUpdateStatus(Status::Processing);
        reportProgress(95);

        if (getOTAState().isUpdateStarted() && Update.end(true)) {
            LOGF(INFO,
                 "OTA filesystem update completed successfully: %d bytes written to partition '%s'",
                 getOTAState().getTotalSize(),
                 FILESYSTEM_PARTITION_LABEL);
            endSessionSuccess();
            request->send(200,
                          "application/json",
                          R"({"success": true, "message": "Filesystem update successful. Device will restart."})");
            scheduleRestart(OTA_RESTART_DELAY_MS);
        } else {
            LOGF(ERROR, "OTA filesystem update failed to finalize: %s", Update.errorString());
            endSessionError("OTA filesystem update failed to finalize: " + String(Update.errorString()));
            const String errorResponse =
                "{\"success\": false, \"message\": \"" + getOTAState().getErrorMessage() + "\"}";
            request->send(500, "application/json", errorResponse);
        }
    }
}

void handleURLUpdate(AsyncWebServerRequest* request) {
    String updateTypeParam = Type::Firmware;
    if (request->hasParam("type", true)) {
        updateTypeParam = request->getParam("type", true)->value();
    }

    if (!request->hasParam("url", true)) {
        request->send(400, "application/json", R"({"success": false, "message": "URL parameter missing"})");
        return;
    }

    const String updateUrl = request->getParam("url", true)->value();

    if (updateUrl.isEmpty()) {
        request->send(400, "application/json", R"({"success": false, "message": "Empty URL provided"})");
        return;
    }

    if (isUpdateInProgress()) {
        LOGF(WARNING,
             "Rejected concurrent OTA URL update attempt (current status: %s, updateStarted: %s, Update.isRunning(): "
             "%s)",
             getOTAState().getUpdateStatus().c_str(),
             getOTAState().isUpdateStarted() ? "true" : "false",
             Update.isRunning() ? "true" : "false");
        request->send(
            409,
            "application/json",
            R"({"success": false, "message": "OTA update already in progress. Please wait for current update to complete."})");
        return;
    }

    LOGF(INFO, "Starting OTA update from URL: %s (type: %s)", updateUrl.c_str(), updateTypeParam.c_str());

    const bool isFilesystem = updateTypeParam == Type::Filesystem;
    beginSession(isFilesystem ? Type::Filesystem : Type::Firmware);
    getOTAState().setUpdateStatus(Status::Downloading);

    const bool success = isFilesystem ? updateFilesystemFromURL(updateUrl) : updateFromURL(updateUrl);

    if (success) {
        endSessionSuccess();
        request->send(
            200, "application/json", R"({"success": true, "message": "Update successful. Device will restart."})");
        scheduleRestart(OTA_RESTART_DELAY_MS);
    } else {
        if (!getOTAState().hasUpdateError()) {
            endSessionError("OTA URL update failed");
        } else {
            endSessionError(getOTAState().getErrorMessage());
        }
        const String response = "{\"success\": false, \"message\": \"" + getOTAState().getErrorMessage() + "\"}";
        request->send(500, "application/json", response);
    }
}

void handleStatus(AsyncWebServerRequest* request) {
    auto&       state      = getOTAState();
    const auto& status     = state.getUpdateStatus();
    const auto  lastUpdate = state.getLastStatusUpdate();
    if ((status == Status::Complete || status == Status::Error) && lastUpdate > 0 && (millis() - lastUpdate > 30000)) {
        resetStatus();
    }

    JsonDocument doc;
    doc["updating"]            = Update.isRunning() || state.isUpdateStarted();
    doc["updateInProgress"]    = isUpdateInProgress();
    doc["progress"]            = state.getCurrentProgress();
    doc["status"]              = state.getUpdateStatus();
    doc["type"]                = state.getUpdateType();
    doc["uploadedSize"]        = state.getUploadedSize();
    doc["totalSize"]           = state.getTotalSize();
    doc["filesystemPartition"] = FILESYSTEM_PARTITION_LABEL;

    if (state.hasUpdateError() || Update.hasError()) {
        const auto& message = state.getErrorMessage();
        if (!message.isEmpty()) {
            doc["error"] = message;
        } else {
            doc["error"] = Update.errorString();
        }
    }

    String response;
    serializeJson(doc, response);
    request->send(200, "application/json", response);
}

uint8_t getProgress() {
    return getOTAState().getCurrentProgress();
}

bool isRunning() {
    return Update.isRunning() || getOTAState().isUpdateStarted();
}

bool isUpdateInProgress() {
    return getOTAState().isUpdateInProgress();
}

const char* getFilesystemPartitionLabel() {
    return FILESYSTEM_PARTITION_LABEL;
}

bool hasError() {
    return getOTAState().hasUpdateError() || Update.hasError();
}

String getErrorMessage() {
    const auto& message = getOTAState().getErrorMessage();
    if (!message.isEmpty()) {
        return message;
    }
    return Update.errorString();
}

const String& getUpdateStatus() noexcept {
    return getOTAState().getUpdateStatus();
}

const String& getUpdateType() noexcept {
    return getOTAState().getUpdateType();
}

bool shouldShowOtaDisplay() noexcept {
    auto& state = getOTAState();

    if (Update.isRunning() || state.isUpdateStarted()) {
        return true;
    }

    const auto& status = state.getUpdateStatus();
    if (status == Status::Uploading || status == Status::Downloading || status == Status::Processing) {
        return true;
    }

    if (state.getLastStatusUpdate() == 0) {
        return false;
    }

    const unsigned long ageMs = millis() - state.getLastStatusUpdate();
    if (status == Status::Error && ageMs < OTA_ERROR_DISPLAY_MS) {
        return true;
    }
    if (status == Status::Complete && ageMs < OTA_COMPLETE_DISPLAY_MS) {
        return true;
    }

    return false;
}

void setup(AsyncWebServer& server) {
    LOGF(INFO, "Setting up OTA endpoints");

    server.on(
        "/api/ota/firmware",
        HTTP_POST,
        [](AsyncWebServerRequest* request) {
            LOGF(ERROR, "OTA firmware upload request received without file data");
            request->send(400, "application/json", R"({"success": false, "message": "No firmware file provided"})");
        },
        handleFirmwareUpload);

    server.on(
        "/api/ota/filesystem",
        HTTP_POST,
        [](AsyncWebServerRequest* request) {
            LOGF(ERROR, "OTA filesystem upload request received without file data");
            request->send(400, "application/json", R"({"success": false, "message": "No filesystem file provided"})");
        },
        handleFilesystemUpload);

    server.on("/api/ota/url", HTTP_POST, handleURLUpdate);
    server.on("/api/ota/status", HTTP_GET, handleStatus);

    LOG(INFO, "OTA endpoints registered successfully");
}

} // namespace OTA
