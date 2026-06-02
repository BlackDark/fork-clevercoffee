/**
 * @file ota.h
 * @brief Over-the-Air (OTA) firmware and filesystem update functionality
 */

#pragma once

#include <Arduino.h>
#include <ESPAsyncWebServer.h>
#include <HTTPClient.h>
#include <Update.h>
#include <WiFi.h>

namespace CleverCoffee {
class SystemContext;
}

class Watchdog;

namespace OTA {

namespace Status {
constexpr const char* Idle        = "idle";
constexpr const char* Uploading   = "uploading";
constexpr const char* Downloading = "downloading";
constexpr const char* Processing  = "processing";
constexpr const char* Complete    = "complete";
constexpr const char* Error       = "error";
} // namespace Status

namespace Type {
constexpr const char* Firmware   = "firmware";
constexpr const char* Filesystem = "filesystem";
} // namespace Type

struct OtaSessionCallbacks {
    void (*prepareHardware)() noexcept = nullptr;
    void (*restoreHardware)() noexcept = nullptr;
};

class OTAStateManager {
  public:
    static OTAStateManager& getInstance() noexcept {
        static OTAStateManager instance;
        return instance;
    }

    OTAStateManager(const OTAStateManager&)            = delete;
    OTAStateManager& operator=(const OTAStateManager&) = delete;
    OTAStateManager(OTAStateManager&&)                 = delete;
    OTAStateManager& operator=(OTAStateManager&&)      = delete;

    [[nodiscard]] bool isUpdateStarted() const noexcept {
        return updateStarted_;
    }
    [[nodiscard]] size_t getTotalSize() const noexcept {
        return totalSize_;
    }
    [[nodiscard]] size_t getUploadedSize() const noexcept {
        return uploadedSize_;
    }
    [[nodiscard]] uint8_t getCurrentProgress() const noexcept {
        return currentProgress_;
    }
    [[nodiscard]] bool hasUpdateError() const noexcept {
        return updateError_;
    }
    [[nodiscard]] const String& getErrorMessage() const noexcept {
        return errorMessage_;
    }
    [[nodiscard]] const String& getUpdateStatus() const noexcept {
        return updateStatus_;
    }
    [[nodiscard]] const String& getUpdateType() const noexcept {
        return updateType_;
    }
    [[nodiscard]] unsigned long getLastStatusUpdate() const noexcept {
        return lastStatusUpdate_;
    }

    void setUpdateStarted(bool started) noexcept {
        updateStarted_ = started;
        if (started) {
            lastStatusUpdate_ = millis();
        }
    }
    void setTotalSize(size_t size) noexcept {
        totalSize_ = size;
    }
    void addUploadedSize(size_t size) noexcept {
        uploadedSize_     += size;
        lastStatusUpdate_  = millis();
    }
    void setCurrentProgress(uint8_t progress) noexcept {
        currentProgress_  = progress;
        lastStatusUpdate_ = millis();
    }
    void setUpdateError(bool error, const String& message = "") noexcept {
        updateError_  = error;
        errorMessage_ = message;
        if (error) {
            updateStatus_     = Status::Error;
            lastStatusUpdate_ = millis();
        }
    }
    void setUpdateStatus(const String& status) noexcept {
        updateStatus_     = status;
        lastStatusUpdate_ = millis();
    }
    void setUpdateType(const String& type) noexcept {
        updateType_ = type;
    }

    void resetState() noexcept {
        updateStarted_   = false;
        totalSize_       = 0;
        uploadedSize_    = 0;
        currentProgress_ = 0;
        updateError_     = false;
        errorMessage_.clear();
        updateStatus_ = Status::Idle;
        updateType_.clear();
        lastStatusUpdate_ = 0;
    }

    [[nodiscard]] bool isUpdateInProgress() const noexcept {
        return updateStarted_ || Update.isRunning() ||
               (updateStatus_ != Status::Idle && updateStatus_ != Status::Complete && updateStatus_ != Status::Error);
    }

  private:
    OTAStateManager() noexcept {
        resetState();
    }

    ~OTAStateManager() = default;

    bool          updateStarted_{false};
    size_t        totalSize_{0};
    size_t        uploadedSize_{0};
    uint8_t       currentProgress_{0};
    bool          updateError_{false};
    String        errorMessage_{};
    String        updateStatus_{Status::Idle};
    String        updateType_{};
    unsigned long lastStatusUpdate_{0};
};

void setup(AsyncWebServer& server);

void setWatchdog(Watchdog* watchdog) noexcept;
void setDisplayContext(CleverCoffee::SystemContext* context) noexcept;
void setSessionCallbacks(OtaSessionCallbacks callbacks) noexcept;

void beginSession(const char* updateType) noexcept;
void endSessionSuccess() noexcept;
void endSessionError(const String& message) noexcept;

void reportProgress(uint8_t percent) noexcept;

[[nodiscard]] bool isActive() noexcept;
void               runMainLoopTick() noexcept;

void scheduleRestart(unsigned long delayMs = 1000) noexcept;
void pollPendingRestart() noexcept;

void refreshDisplay(CleverCoffee::SystemContext& context) noexcept;

void initializeArduinoOta(const char* hostname, const char* password);

bool updateFromURL(const String& url);

void handleFirmwareUpload(
    AsyncWebServerRequest* request, const String& filename, size_t index, uint8_t* data, size_t len, bool final);

void handleFilesystemUpload(
    AsyncWebServerRequest* request, const String& filename, size_t index, uint8_t* data, size_t len, bool final);

void handleURLUpdate(AsyncWebServerRequest* request);
void handleStatus(AsyncWebServerRequest* request);

[[nodiscard]] uint8_t       getProgress();
[[nodiscard]] bool          isRunning();
[[nodiscard]] bool          isUpdateInProgress();
[[nodiscard]] const char*   getFilesystemPartitionLabel();
[[nodiscard]] bool          hasError();
[[nodiscard]] String        getErrorMessage();
[[nodiscard]] const String& getUpdateStatus() noexcept;
[[nodiscard]] const String& getUpdateType() noexcept;
[[nodiscard]] bool          shouldShowOtaDisplay() noexcept;

} // namespace OTA
