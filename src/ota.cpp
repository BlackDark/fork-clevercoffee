/**
 * @file ota.cpp
 * @brief Over-the-Air (OTA) firmware update implementation
 */

#include "ota.h"
#include "Logger.h"
#include "utils/helperUtils.h"
#include <ArduinoJson.h>

namespace OTA {

    // Use modern RAII singleton for state management
    namespace {
        inline auto& getOTAState() noexcept {
            return OTAStateManager::getInstance();
        }
    }

    // Legacy static variables removed - using OTAStateManager instead

    // Filesystem partition label (SPIFFS partition name)
    // This should match the partition table configuration
    // Common labels: "spiffs", "littlefs", "fatfs"
    // To change this label, update both this value and your partition table (partitions.csv)
    static const char* FILESYSTEM_PARTITION_LABEL = "spiffs";

    void resetStatus() {
        getOTAState().resetState();
    }

    bool updateFromURL(const String& url) {
        if (url.isEmpty()) {
            LOG(ERROR, "OTA update URL is empty");
            getOTAState().setUpdateError(true, "OTA update URL is empty");
            return false;
        }

        LOGF(INFO, "Starting OTA update from URL: %s", url.c_str());

        // Reset error state
        auto& state = getOTAState();
        state.setUpdateError(false);
        state.setCurrentProgress(0);
        state.setUpdateStatus("downloading");

        WiFiClient client;
        HTTPClient http;

        http.begin(client, url);
        http.addHeader("User-Agent", "ESP32-CleverCoffee-OTA");

        int httpCode = http.GET();

        if (httpCode != HTTP_CODE_OK) {
            LOGF(ERROR, "HTTP GET failed, error: %d", httpCode);
            getOTAState().setUpdateError(true, "HTTP GET failed, error: " + String(httpCode));
            http.end();
            return false;
        }

        int contentLength = http.getSize();

        if (contentLength <= 0) {
            LOG(ERROR, "Content-Length not found or zero");
            getOTAState().setUpdateError(true, "Content-Length not found or zero");
            http.end();
            return false;
        }

        LOGF(INFO, "Starting OTA update, firmware size: %d bytes", contentLength);

        if (!Update.begin(contentLength)) {
            LOGF(ERROR, "Cannot begin OTA update: %s", Update.errorString());
            getOTAState().setUpdateError(true, "Cannot begin OTA update: " + String(Update.errorString()));
            http.end();
            return false;
        }

        WiFiClient* stream = http.getStreamPtr();
        size_t written = 0;
        uint8_t buffer[OTA_BUFFER_SIZE];

        while (http.connected() && (written < contentLength)) {
            size_t available = stream->available();
            if (available > 0) {
                size_t toRead = min(available, sizeof(buffer));
                size_t bytesRead = stream->readBytes(buffer, toRead);

                if (Update.write(buffer, bytesRead) != bytesRead) {
                    LOGF(ERROR, "Write failed at byte %d", written);
                    getOTAState().setUpdateError(true, "Write failed at byte " + String(written));
                    Update.abort();
                    http.end();
                    return false;
                }

                written += bytesRead;

                // Update progress
                getOTAState().setCurrentProgress((written * 100) / contentLength);

                // Log progress every 10%
                if (written % (contentLength / 10) == 0) {
                    LOGF(INFO, "OTA Progress: %d%%", getOTAState().getCurrentProgress());
                }
            }
            delay(1);
        }

        http.end();

        if (written != contentLength) {
            LOGF(ERROR, "Written %d bytes, expected %d", written, contentLength);
            getOTAState().setUpdateError(true, "Incomplete download: written " + String(written) + " bytes, expected " + String(contentLength));
            Update.abort();
            return false;
        }

        if (Update.end()) {
            if (Update.isFinished()) {
                LOG(INFO, "OTA update completed successfully");
                getOTAState().setCurrentProgress(100);
                getOTAState().setUpdateStatus("complete");
                return true;
            }
            else {
                LOGF(ERROR, "OTA update not finished: %s", Update.errorString());
                getOTAState().setUpdateError(true, "OTA update not finished: " + String(Update.errorString()));
                return false;
            }
        }
        else {
            LOGF(ERROR, "OTA update failed: %s", Update.errorString());
            getOTAState().setUpdateError(true, "OTA update failed: " + String(Update.errorString()));
            return false;
        }
    }

    // Helper for filesystem update via URL
    bool updateFilesystemFromURL(const String& url) {
        if (url.isEmpty()) {
            LOG(ERROR, "OTA filesystem update URL is empty");
            getOTAState().setUpdateError(true, "OTA filesystem update URL is empty");
            return false;
        }

        LOGF(INFO, "Starting OTA filesystem update from URL: %s", url.c_str());

        // Reset error state
        getOTAState().setUpdateError(false);
        getOTAState().setCurrentProgress(0);
        getOTAState().setUpdateStatus("downloading");

        WiFiClient client;
        HTTPClient http;

        http.begin(client, url);
        http.addHeader("User-Agent", "ESP32-CleverCoffee-OTA");

        int httpCode = http.GET();

        if (httpCode != HTTP_CODE_OK) {
            LOGF(ERROR, "HTTP GET failed, error: %d", httpCode);
            getOTAState().setUpdateError(true, "HTTP GET failed, error: " + String(httpCode));
            http.end();
            return false;
        }

        int contentLength = http.getSize();

        if (contentLength <= 0) {
            LOG(ERROR, "Content-Length not found or zero");
            getOTAState().setUpdateError(true, "Content-Length not found or zero");
            http.end();
            return false;
        }

        // Validate extension
        String path = url;
        path.toLowerCase();
        if (!path.endsWith(".bin") && !path.endsWith(".img")) {
            LOGF(ERROR, "Invalid filesystem file extension: %s", path.c_str());
            getOTAState().setUpdateError(true, "Invalid filesystem file extension (.bin or .img required)");
            http.end();
            return false;
        }

        LOGF(INFO, "Starting OTA filesystem update, size: %d bytes", contentLength);
        if (!Update.begin(UPDATE_SIZE_UNKNOWN, U_SPIFFS, -1, LOW, FILESYSTEM_PARTITION_LABEL)) {
            LOGF(ERROR, "Cannot begin OTA filesystem update: %s", Update.errorString());
            getOTAState().setUpdateError(true, "Cannot begin OTA filesystem update: " + String(Update.errorString()));
            http.end();
            return false;
        }

        WiFiClient* stream = http.getStreamPtr();
        size_t written = 0;
        uint8_t buffer[OTA_BUFFER_SIZE];

        while (http.connected() && (written < contentLength)) {
            size_t available = stream->available();
            if (available > 0) {
                size_t toRead = min(available, sizeof(buffer));
                size_t bytesRead = stream->readBytes(buffer, toRead);

                if (Update.write(buffer, bytesRead) != bytesRead) {
                    LOGF(ERROR, "Filesystem write failed at byte %d", written);
                    getOTAState().setUpdateError(true, "Filesystem write failed at byte " + String(written));
                    Update.abort();
                    http.end();
                    return false;
                }

                written += bytesRead;
                getOTAState().setCurrentProgress((written * 100) / contentLength);
                if (written % (contentLength / 10) == 0) {
                    LOGF(INFO, "Filesystem OTA Progress: %d%%", getOTAState().getCurrentProgress());
                }
            }
            delay(1);
        }

        http.end();

        if (written != contentLength) {
            LOGF(ERROR, "Written %d bytes, expected %d", written, contentLength);
            getOTAState().setUpdateError(true, "Incomplete download: written " + String(written) + " bytes, expected " + String(contentLength));
            Update.abort();
            return false;
        }

        if (Update.end(true)) {
            if (Update.isFinished()) {
                LOG(INFO, "OTA filesystem update completed successfully");
                getOTAState().setCurrentProgress(100);
                getOTAState().setUpdateStatus("complete");
                return true;
            }
            else {
                LOGF(ERROR, "OTA filesystem update not finished: %s", Update.errorString());
                getOTAState().setUpdateError(true, "OTA filesystem update not finished: " + String(Update.errorString()));
                return false;
            }
        }
        else {
            LOGF(ERROR, "OTA filesystem update failed: %s", Update.errorString());
            getOTAState().setUpdateError(true, "OTA filesystem update failed: " + String(Update.errorString()));
            return false;
        }
    }

    void handleFirmwareUpload(AsyncWebServerRequest* request, const String& filename, size_t index, uint8_t* data, size_t len, bool final) {
        // Log every invocation for debugging
        LOGF(INFO, "handleFirmwareUpload called: filename=%s, index=%d, len=%d, final=%d", filename.c_str(), static_cast<int>(index), static_cast<int>(len), final);

        // Validate that this is a firmware file (check file extension) - only on first chunk
        if (index == 0 && !filename.endsWith(".bin")) {
            LOGF(ERROR, "Invalid firmware file: %s (expected .bin extension)", filename.c_str());
            request->send(400, "application/json", R"({"success": false, "message": "Invalid firmware file. Expected .bin extension."})");
            return;
        }

        if (index == 0) {
            // Check if an OTA update is already in progress
            if (isUpdateInProgress()) {
                LOGF(WARNING, "Rejected concurrent OTA file upload attempt (current status: %s, updateStarted: %s, Update.isRunning(): %s)", 
                     getOTAState().getUpdateStatus().c_str(), getOTAState().isUpdateStarted() ? "true" : "false",
                     Update.isRunning() ? "true" : "false");
                request->send(409, "application/json", R"({"success": false, "message": "OTA update already in progress. Please wait for current update to complete."})");
                return;
            }

            // Reset status for new upload
            resetStatus();
            getOTAState().setUpdateStatus("uploading");
            getOTAState().setUpdateType("firmware");

            LOGF(INFO, "OTA firmware update started: %s", filename.c_str());

            // Stop all operations for safe OTA update
            if (!Update.begin(UPDATE_SIZE_UNKNOWN)) {
                LOGF(ERROR, "OTA update failed to begin: %s", Update.errorString());
                getOTAState().setUpdateError(true, "OTA update failed to begin: " + String(Update.errorString()));
                request->send(500, "application/json", R"({"success": false, "message": "Failed to begin update"})");
                return;
            }
            getOTAState().setUpdateStarted(true);
        }

        if (getOTAState().isUpdateStarted()) {
            if (Update.write(data, len) != len) {
                LOGF(ERROR, "OTA update write failed at byte %d", index + len);
                getOTAState().setUpdateError(true, "OTA update write failed at byte " + String(index + len));
                Update.abort();
                getOTAState().setUpdateStarted(false);
                request->send(500, "application/json", R"({"success": false, "message": "Write failed"})");
                return;
            }
            getOTAState().addUploadedSize(len);
            getOTAState().setTotalSize(index + len); // Track total uploaded so far

            // Update progress - more accurate calculation
            // For file uploads, we can't know the total size in advance, so estimate based on uploaded data
            if (getOTAState().getUploadedSize() > 0) {
                // Show progress up to 90% during upload, reserve 10% for processing
                getOTAState().setCurrentProgress(min(90, (int)((getOTAState().getUploadedSize() * 90) / max(getOTAState().getUploadedSize(), (size_t)(512 * 1024))))) ; // Assume min 512KB firmware
            }
        }

        if (final) {
            getOTAState().setUpdateStatus("processing");
            getOTAState().setCurrentProgress(95);

            if (getOTAState().isUpdateStarted() && Update.end(true)) {
                LOGF(INFO, "OTA firmware update completed successfully: %d bytes", getOTAState().getTotalSize());
                getOTAState().setCurrentProgress(100);
                getOTAState().setUpdateStatus("complete");
                request->send(200, "application/json", R"({"success": true, "message": "Update successful. Device will restart."})");
                delay(1000);
                ESP.restart();
            }
            else {
                LOGF(ERROR, "OTA update failed to finalize: %s", Update.errorString());
                getOTAState().setUpdateError(true, "OTA update failed to finalize: " + String(Update.errorString()));
                String errorResponse = "{\"success\": false, \"message\": \"" + getOTAState().getErrorMessage() + "\"}";
                request->send(500, "application/json", errorResponse);
            }
            getOTAState().setUpdateStarted(false);
        }
    }

    void handleFilesystemUpload(AsyncWebServerRequest* request, const String& filename, size_t index, uint8_t* data, size_t len, bool final) {
        // Log every invocation for debugging
        LOGF(INFO, "handleFilesystemUpload called: filename=%s, index=%d, len=%d, final=%d", filename.c_str(), static_cast<int>(index), static_cast<int>(len), final);

        // Validate that this is a filesystem file (check file extension) - only on first chunk
        if (index == 0 && !filename.endsWith(".bin") && !filename.endsWith(".img")) {
            LOGF(ERROR, "Invalid filesystem file: %s (expected .bin or .img extension)", filename.c_str());
            request->send(400, "application/json", R"({"success": false, "message": "Invalid filesystem file. Expected .bin or .img extension."})");
            return;
        }

        if (index == 0) {
            // Check if an OTA update is already in progress
            if (isUpdateInProgress()) {
                LOGF(WARNING, "Rejected concurrent OTA filesystem upload attempt (current status: %s, updateStarted: %s, Update.isRunning(): %s)", 
                     getOTAState().getUpdateStatus().c_str(), getOTAState().isUpdateStarted() ? "true" : "false",
                     Update.isRunning() ? "true" : "false");
                request->send(409, "application/json", R"({"success": false, "message": "OTA update already in progress. Please wait for current update to complete."})");
                return;
            }

            // Reset status for new upload
            resetStatus();
            getOTAState().setUpdateStatus("uploading");
            getOTAState().setUpdateType("filesystem");

            LOGF(INFO, "OTA filesystem update started: %s (partition: %s)", filename.c_str(), FILESYSTEM_PARTITION_LABEL);

            // Stop all operations for safe OTA update - use SPIFFS partition type for filesystem
            // Begin update with filesystem partition label
            if (!Update.begin(UPDATE_SIZE_UNKNOWN, U_SPIFFS, -1, LOW, FILESYSTEM_PARTITION_LABEL)) {
                LOGF(ERROR, "OTA filesystem update failed to begin on partition '%s': %s", FILESYSTEM_PARTITION_LABEL, Update.errorString());
                getOTAState().setUpdateError(true, "OTA filesystem update failed to begin: " + String(Update.errorString()));
                request->send(500, "application/json", R"({"success": false, "message": "Failed to begin filesystem update"})");
                return;
            }
            getOTAState().setUpdateStarted(true);
        }

        if (getOTAState().isUpdateStarted()) {
            if (Update.write(data, len) != len) {
                LOGF(ERROR, "OTA filesystem update write failed at byte %d", index + len);
                getOTAState().setUpdateError(true, "OTA filesystem update write failed at byte " + String(index + len));
                Update.abort();
                getOTAState().setUpdateStarted(false);
                request->send(500, "application/json", R"({"success": false, "message": "Filesystem write failed"})");
                return;
            }
            getOTAState().addUploadedSize(len);
            getOTAState().setTotalSize(index + len); // Track total uploaded so far

            // Update progress - more accurate calculation
            // For file uploads, we can't know the total size in advance, so estimate based on uploaded data
            if (getOTAState().getUploadedSize() > 0) {
                // Show progress up to 90% during upload, reserve 10% for processing
                getOTAState().setCurrentProgress(min(90, (int)((getOTAState().getUploadedSize() * 90) / max(getOTAState().getUploadedSize(), (size_t)(256 * 1024))))); // Assume min 256KB filesystem
            }
        }

        if (final) {
            getOTAState().setUpdateStatus("processing");
            getOTAState().setCurrentProgress(95);

            if (getOTAState().isUpdateStarted() && Update.end(true)) {
                LOGF(INFO, "OTA filesystem update completed successfully: %d bytes written to partition '%s'", getOTAState().getTotalSize(), FILESYSTEM_PARTITION_LABEL);
                getOTAState().setCurrentProgress(100);
                getOTAState().setUpdateStatus("complete");
                request->send(200, "application/json", R"({"success": true, "message": "Filesystem update successful. Device will restart."})");
                delay(1000);
                ESP.restart();
            }
            else {
                LOGF(ERROR, "OTA filesystem update failed to finalize: %s", Update.errorString());
                getOTAState().setUpdateError(true, "OTA filesystem update failed to finalize: " + String(Update.errorString()));
                String errorResponse = "{\"success\": false, \"message\": \"" + getOTAState().getErrorMessage() + "\"}";
                request->send(500, "application/json", errorResponse);
            }
            getOTAState().setUpdateStarted(false);
        }
    }

    void handleURLUpdate(AsyncWebServerRequest* request) {
        String updateTypeParam = "firmware";
        if (request->hasParam("type", true)) {
            updateTypeParam = request->getParam("type", true)->value();
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

        // Check if an OTA update is already in progress
        if (isUpdateInProgress()) {
            LOGF(WARNING, "Rejected concurrent OTA URL update attempt (current status: %s, updateStarted: %s, Update.isRunning(): %s)", 
                 getOTAState().getUpdateStatus().c_str(), getOTAState().isUpdateStarted() ? "true" : "false",
                 Update.isRunning() ? "true" : "false");
            request->send(409, "application/json", R"({"success": false, "message": "OTA update already in progress. Please wait for current update to complete."})");
            return;
        }

        LOGF(INFO, "Starting OTA update from URL: %s (type: %s)", updateUrl.c_str(), updateTypeParam.c_str());

        // Reset status for new update
        resetStatus();
        getOTAState().setUpdateType(updateTypeParam);
        getOTAState().setUpdateStatus("downloading");

        bool success = false;
        if (updateTypeParam == "filesystem") {
            success = updateFilesystemFromURL(updateUrl);
        }
        else {
            success = updateFromURL(updateUrl);
        }

        if (success) {
            request->send(200, "application/json", R"({"success": true, "message": "Update successful. Device will restart."})");
            delay(2000);
            ESP.restart();
        }
        else {
            String response = "{\"success\": false, \"message\": \"" + getOTAState().getErrorMessage() + "\"}";
            request->send(500, "application/json", response);
        }
    }

    void handleStatus(AsyncWebServerRequest* request) {
        // Auto-reset status after 30 seconds of inactivity for completed/error states (internal cleanup)
        auto& state = getOTAState();
        const auto& status = state.getUpdateStatus();
        auto lastUpdate = state.getLastStatusUpdate();
        if ((status == "complete" || status == "error") && lastUpdate > 0 && (millis() - lastUpdate > 30000)) {
            resetStatus();
        }

        JsonDocument doc;
        doc["updating"] = Update.isRunning() || state.isUpdateStarted();
        doc["updateInProgress"] = isUpdateInProgress();
        doc["progress"] = state.getCurrentProgress();
        doc["status"] = state.getUpdateStatus();
        doc["type"] = state.getUpdateType();
        doc["uploadedSize"] = state.getUploadedSize();
        doc["totalSize"] = state.getTotalSize();
        doc["filesystemPartition"] = FILESYSTEM_PARTITION_LABEL;

        if (state.hasUpdateError() || Update.hasError()) {
            const auto& message = state.getErrorMessage();
            if (!message.isEmpty()) {
                doc["error"] = message;
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

    void setup(AsyncWebServer& server) {
        LOGF(INFO, "Setting up OTA endpoints");

        // OTA Update via firmware file upload
        server.on(
            "/api/ota/firmware", HTTP_POST,
            [](AsyncWebServerRequest* request) {
                // This response will be set by the upload handler
                // If we reach here without upload handler being called, it's an error
                LOGF(ERROR, "OTA firmware upload request received without file data");
                request->send(400, "application/json", R"({"success": false, "message": "No firmware file provided"})");
            },
            handleFirmwareUpload);

        // OTA Update via filesystem file upload
        server.on(
            "/api/ota/filesystem", HTTP_POST,
            [](AsyncWebServerRequest* request) {
                // This response will be set by the upload handler
                // If we reach here without upload handler being called, it's an error
                LOGF(ERROR, "OTA filesystem upload request received without file data");
                request->send(400, "application/json", R"({"success": false, "message": "No filesystem file provided"})");
            },
            handleFilesystemUpload);

        // OTA Update via URL
        server.on("/api/ota/url", HTTP_POST, handleURLUpdate);

        // OTA Update status endpoint
        server.on("/api/ota/status", HTTP_GET, handleStatus);

        LOG(INFO, "OTA endpoints registered successfully");
    }

} // namespace OTA
