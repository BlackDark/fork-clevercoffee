/**
 * @file ota.h
 * @brief Over-the-Air (OTA) firmware update functionality
 */

#pragma once

#include <Arduino.h>
#include <ESPAsyncWebServer.h>
#include <HTTPClient.h>
#include <Update.h>
#include <WiFi.h>

namespace OTA {

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
     * @brief Handle file upload for OTA updates
     * @param request The web server request
     * @param filename Name of the uploaded file
     * @param index Current position in the file
     * @param data Chunk of file data
     * @param len Length of the data chunk
     * @param final True if this is the last chunk
     */
    void handleFileUpload(AsyncWebServerRequest* request, const String& filename, size_t index, uint8_t* data, size_t len, bool final);

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
