/**
 * @file ota.h
 * @brief Over-the-Air (OTA) firmware and filesystem update functionality
 *
 * Supports both firmware updates (to app partition) and filesystem updates (to SPIFFS partition).
 * The filesystem partition label is configured to "spiffs" and should match the partition table.
 */

#pragma once

#include <Arduino.h>
#include <ESPAsyncWebServer.h>
#include <HTTPClient.h>
#include <Update.h>
#include <WiFi.h>

namespace OTA {

#if __cplusplus >= 202002L // C++20 and later for enhanced singleton pattern

    /**
     * @brief Modern C++23 RAII singleton for OTA state management
     * Replaces static variables with proper resource management and thread safety
     */
    class OTAStateManager {
    public:
        // Thread-safe singleton with guaranteed initialization
        static OTAStateManager& getInstance() noexcept {
            static OTAStateManager instance;
            return instance;
        }
        
        // Delete copy/move constructors and assignment operators
        OTAStateManager(const OTAStateManager&) = delete;
        OTAStateManager& operator=(const OTAStateManager&) = delete;
        OTAStateManager(OTAStateManager&&) = delete;
        OTAStateManager& operator=(OTAStateManager&&) = delete;
        
        // State access methods with proper encapsulation
        [[nodiscard]] bool isUpdateStarted() const noexcept { return updateStarted_; }
        [[nodiscard]] size_t getTotalSize() const noexcept { return totalSize_; }
        [[nodiscard]] size_t getUploadedSize() const noexcept { return uploadedSize_; }
        [[nodiscard]] uint8_t getCurrentProgress() const noexcept { return currentProgress_; }
        [[nodiscard]] bool hasUpdateError() const noexcept { return updateError_; }
        [[nodiscard]] const String& getErrorMessage() const noexcept { return errorMessage_; }
        [[nodiscard]] const String& getUpdateStatus() const noexcept { return updateStatus_; }
        [[nodiscard]] const String& getUpdateType() const noexcept { return updateType_; }
        [[nodiscard]] unsigned long getLastStatusUpdate() const noexcept { return lastStatusUpdate_; }
        
        // State modification methods
        void setUpdateStarted(bool started) noexcept { 
            updateStarted_ = started; 
            if (started) lastStatusUpdate_ = millis();
        }
        void setTotalSize(size_t size) noexcept { totalSize_ = size; }
        void addUploadedSize(size_t size) noexcept { 
            uploadedSize_ += size; 
            lastStatusUpdate_ = millis();
        }
        void setCurrentProgress(uint8_t progress) noexcept { 
            currentProgress_ = progress; 
            lastStatusUpdate_ = millis();
        }
        void setUpdateError(bool error, const String& message = "") noexcept { 
            updateError_ = error; 
            errorMessage_ = message; 
            if (error) {
                updateStatus_ = "error";
                lastStatusUpdate_ = millis();
            }
        }
        void setUpdateStatus(const String& status) noexcept { 
            updateStatus_ = status; 
            lastStatusUpdate_ = millis();
        }
        void setUpdateType(const String& type) noexcept { updateType_ = type; }
        
        // Reset all state to initial values
        void resetState() noexcept {
            updateStarted_ = false;
            totalSize_ = 0;
            uploadedSize_ = 0;
            currentProgress_ = 0;
            updateError_ = false;
            errorMessage_.clear();
            updateStatus_ = "idle";
            updateType_.clear();
            lastStatusUpdate_ = 0;
        }
        
        // Convenience method to check if any update is in progress
        [[nodiscard]] bool isUpdateInProgress() const noexcept {
            return updateStarted_ || Update.isRunning() || 
                   (updateStatus_ != "idle" && updateStatus_ != "complete" && updateStatus_ != "error");
        }
        
    private:
        // Private constructor for singleton pattern
        OTAStateManager() noexcept {
            resetState();
        }
        
        ~OTAStateManager() = default;
        
        // State variables (previously static)
        bool updateStarted_{false};
        size_t totalSize_{0};
        size_t uploadedSize_{0};
        uint8_t currentProgress_{0};
        bool updateError_{false};
        String errorMessage_{};
        String updateStatus_{"idle"};  // idle, uploading, processing, complete, error
        String updateType_{};          // firmware, filesystem, url
        unsigned long lastStatusUpdate_{0};
    };
    
#endif // __cplusplus >= 202002L

    /**
     * @brief Initialize OTA functionality and register web server routes
     * @param server Reference to the AsyncWebServer instance
     */
    void setup(AsyncWebServer& server);

    /**
     * @brief Perform OTA update from a URL
     * @param url The URL to download firmware from
     * @return true if update was successful, false otherwise
     */
    bool updateFromURL(const String& url);

    /**
     * @brief Handle firmware file upload for OTA updates
     * @param request The web server request
     * @param filename Name of the uploaded file
     * @param index Current position in the file
     * @param data Chunk of file data
     * @param len Length of the data chunk
     * @param final True if this is the last chunk
     */
    void handleFirmwareUpload(AsyncWebServerRequest* request, const String& filename, size_t index, uint8_t* data, size_t len, bool final);

    /**
     * @brief Handle filesystem file upload for OTA updates
     * @param request The web server request
     * @param filename Name of the uploaded file
     * @param index Current position in the file
     * @param data Chunk of file data
     * @param len Length of the data chunk
     * @param final True if this is the last chunk
     */
    void handleFilesystemUpload(AsyncWebServerRequest* request, const String& filename, size_t index, uint8_t* data, size_t len, bool final);

    /**
     * @brief Handle OTA update from URL endpoint
     * @param request The web server request
     */
    void handleURLUpdate(AsyncWebServerRequest* request);

    /**
     * @brief Handle OTA status endpoint
     * @param request The web server request
     */
    void handleStatus(AsyncWebServerRequest* request);

    /**
     * @brief Get current OTA update progress
     * @return Progress percentage (0-100)
     */
    uint8_t getProgress();

    /**
     * @brief Check if OTA update is currently running
     * @return true if update is in progress
     */
    bool isRunning();

    /**
     * @brief Check if any OTA update is in progress (including preparation phases)
     * @return true if update is in progress or being prepared
     */
    bool isUpdateInProgress();

    /**
     * @brief Get the filesystem partition label
     * @return Filesystem partition label string
     */
    const char* getFilesystemPartitionLabel();

    /**
     * @brief Check if there was an OTA error
     * @return true if there was an error
     */
    bool hasError();

    /**
     * @brief Get the last OTA error message
     * @return Error message string
     */
    String getErrorMessage();

} // namespace OTA
