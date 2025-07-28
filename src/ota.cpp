/**
 * @file ota.cpp
 * @brief Over-the-Air (OTA) firmware update implementation
 */

#include "ota.h"
#include "utils/helperUtils.h"
#include <ArduinoJson.h>

// Include embeddedWebserver.h to get access to authenticate function
// Note: This creates a circular dependency but is needed for authentication
bool otaAuthenticate(AsyncWebServerRequest* request);

// Use Serial for logging to avoid conflicts with Logger/WiFiManager
#define LOG(level, msg)          Serial.println(msg)
#define LOGF(level, format, ...) Serial.printf(format "\n", ##__VA_ARGS__)

namespace OTA {

    // Static variables for tracking OTA state
    static bool updateStarted = false;
    static size_t totalSize = 0;
    static uint8_t currentProgress = 0;
    static bool updateError = false;
    static String errorMessage = "";

    // External function for authentication check (defined in embeddedWebserver.h)
    // Forward declaration - actual implementation is in embeddedWebserver.h

    bool updateFromURL(const String& url) {
        if (url.isEmpty()) {
            LOG(ERROR, "OTA update URL is empty");
            errorMessage = "OTA update URL is empty";
            updateError = true;
            return false;
        }

        LOGF(INFO, "Starting OTA update from URL: %s", url.c_str());

        // Reset error state
        updateError = false;
        errorMessage = "";
        currentProgress = 0;

        WiFiClient client;
        HTTPClient http;

        http.begin(client, url);
        http.addHeader("User-Agent", "ESP32-CleverCoffee-OTA");

        int httpCode = http.GET();

        if (httpCode != HTTP_CODE_OK) {
            LOGF(ERROR, "HTTP GET failed, error: %d", httpCode);
            errorMessage = "HTTP GET failed, error: " + String(httpCode);
            updateError = true;
            http.end();
            return false;
        }

        int contentLength = http.getSize();

        if (contentLength <= 0) {
            LOG(ERROR, "Content-Length not found or zero");
            errorMessage = "Content-Length not found or zero";
            updateError = true;
            http.end();
            return false;
        }

        LOGF(INFO, "Starting OTA update, firmware size: %d bytes", contentLength);

        if (!Update.begin(contentLength)) {
            LOGF(ERROR, "Cannot begin OTA update: %s", Update.errorString());
            errorMessage = "Cannot begin OTA update: " + String(Update.errorString());
            updateError = true;
            http.end();
            return false;
        }

        WiFiClient* stream = http.getStreamPtr();
        size_t written = 0;
        uint8_t buffer[1024];

        while (http.connected() && (written < contentLength)) {
            size_t available = stream->available();
            if (available > 0) {
                size_t toRead = min(available, sizeof(buffer));
                size_t bytesRead = stream->readBytes(buffer, toRead);

                if (Update.write(buffer, bytesRead) != bytesRead) {
                    LOGF(ERROR, "Write failed at byte %d", written);
                    errorMessage = "Write failed at byte " + String(written);
                    updateError = true;
                    Update.abort();
                    http.end();
                    return false;
                }

                written += bytesRead;

                // Update progress
                currentProgress = (written * 100) / contentLength;

                // Log progress every 10%
                if (written % (contentLength / 10) == 0) {
                    LOGF(INFO, "OTA Progress: %d%%", currentProgress);
                }
            }
            delay(1);
        }

        http.end();

        if (written != contentLength) {
            LOGF(ERROR, "Written %d bytes, expected %d", written, contentLength);
            errorMessage = "Incomplete download: written " + String(written) + " bytes, expected " + String(contentLength);
            updateError = true;
            Update.abort();
            return false;
        }

        if (Update.end()) {
            if (Update.isFinished()) {
                LOG(INFO, "OTA update completed successfully");
                currentProgress = 100;
                return true;
            }
            else {
                LOGF(ERROR, "OTA update not finished: %s", Update.errorString());
                errorMessage = "OTA update not finished: " + String(Update.errorString());
                updateError = true;
                return false;
            }
        }
        else {
            LOGF(ERROR, "OTA update failed: %s", Update.errorString());
            errorMessage = "OTA update failed: " + String(Update.errorString());
            updateError = true;
            return false;
        }
    }

    void handleFileUpload(AsyncWebServerRequest* request, const String& filename, size_t index, uint8_t* data, size_t len, bool final) {
        if (!otaAuthenticate(request)) {
            return request->requestAuthentication();
        }

        if (index == 0) {
            updateStarted = false;
            totalSize = 0;
            currentProgress = 0;
            updateError = false;
            errorMessage = "";

            LOGF(INFO, "OTA firmware update started: %s", filename.c_str());

            // Stop all operations for safe OTA update
            if (!Update.begin(UPDATE_SIZE_UNKNOWN)) {
                LOGF(ERROR, "OTA update failed to begin: %s", Update.errorString());
                errorMessage = "OTA update failed to begin: " + String(Update.errorString());
                updateError = true;
                request->send(500, "application/json", R"({"success": false, "message": "Failed to begin update"})");
                return;
            }
            updateStarted = true;
        }

        if (updateStarted) {
            if (Update.write(data, len) != len) {
                LOGF(ERROR, "OTA update write failed at byte %d", index + len);
                errorMessage = "OTA update write failed at byte " + String(index + len);
                updateError = true;
                Update.abort();
                updateStarted = false;
                request->send(500, "application/json", R"({"success": false, "message": "Write failed"})");
                return;
            }
            totalSize += len;

            // Update progress - estimate based on typical firmware size
            if (totalSize > 0) {
                currentProgress = min(95, (int)((totalSize * 100) / (1024 * 1024))); // Assume max 1MB firmware
            }
        }

        if (final) {
            if (updateStarted && Update.end(true)) {
                LOGF(INFO, "OTA firmware update completed successfully: %d bytes", totalSize);
                currentProgress = 100;
                request->send(200, "application/json", R"({"success": true, "message": "Update successful. Device will restart."})");
                delay(1000);
                ESP.restart();
            }
            else {
                LOGF(ERROR, "OTA update failed to finalize: %s", Update.errorString());
                errorMessage = "OTA update failed to finalize: " + String(Update.errorString());
                updateError = true;
                request->send(500, "application/json", R"({"success": false, "message": "Update failed to complete"})");
            }
            updateStarted = false;
        }
    }

    void handleURLUpdate(AsyncWebServerRequest* request) {
        if (!otaAuthenticate(request)) {
            return request->requestAuthentication();
        }

        if (!request->hasParam("url", true)) {
            request->send(400, "application/json", R"({"success": false, "message": "URL parameter missing"})");
            return;
        }

        String updateUrl = request->getParam("url", true)->value();

        if (updateUrl.isEmpty()) {
            request->send(400, "application/json", R"({"success": false, "message": "Empty URL provided"})");
            return;
        }

        LOGF(INFO, "Starting OTA update from URL: %s", updateUrl.c_str());

        // Perform the update synchronously (this will block the request)
        bool success = updateFromURL(updateUrl);

        if (success) {
            request->send(200, "application/json", R"({"success": true, "message": "Update successful. Device will restart."})");
            delay(2000);
            ESP.restart();
        }
        else {
            String response = "{\"success\": false, \"message\": \"" + errorMessage + "\"}";
            request->send(500, "application/json", response);
        }
    }

    void handleStatus(AsyncWebServerRequest* request) {
        if (!otaAuthenticate(request)) {
            return request->requestAuthentication();
        }

        JsonDocument doc;
        doc["updating"] = Update.isRunning();
        doc["progress"] = currentProgress;
        doc["size"] = Update.size();

        if (updateError || Update.hasError()) {
            if (!errorMessage.isEmpty()) {
                doc["error"] = errorMessage;
            }
            else {
                doc["error"] = Update.errorString();
            }
        }

        String response;
        serializeJson(doc, response);
        request->send(200, "application/json", response);
    }

    uint8_t getProgress() {
        return currentProgress;
    }

    bool isRunning() {
        return Update.isRunning() || updateStarted;
    }

    bool hasError() {
        return updateError || Update.hasError();
    }

    String getErrorMessage() {
        if (!errorMessage.isEmpty()) {
            return errorMessage;
        }
        return Update.errorString();
    }

    void setup(AsyncWebServer& server) {
        LOGF(INFO, "Setting up OTA endpoints");

        // OTA Update via file upload
        server.on(
            "/api/ota/firmware", HTTP_POST,
            [](AsyncWebServerRequest* request) {
                // This response will be set by the upload handler
            },
            handleFileUpload);

        // OTA Update via URL
        server.on("/api/ota/url", HTTP_POST, handleURLUpdate);

        // OTA Update status endpoint
        server.on("/api/ota/status", HTTP_GET, handleStatus);

        LOG(INFO, "OTA endpoints registered successfully");
    }

} // namespace OTA
