/**
 * @file WebServerManager.h
 * @brief Manages the embedded web server for CleverCoffee
 */

#pragma once

#include <Arduino.h>
#include <ESPAsyncWebServer.h>
#include <LittleFS.h>
#include <memory>

// Forward declarations for middleware classes from ESPAsyncWebServer
class AsyncCorsMiddleware;
class AsyncAuthenticationMiddleware;

// Forward declarations
namespace CleverCoffee {
class SystemContext;
}

/**
 * @class WebServerManager
 * @brief Manages the embedded web server with all routes, middleware, and event handling
 */
class WebServerManager {
  public:
    /**
     * @brief Constructor
     * @param port Server port (default: 80)
     */
    explicit WebServerManager(uint16_t port = 80);

    /**
     * @brief Destructor
     */
    ~WebServerManager();

    /**
     * @brief Initialize and start the web server
     * @param littleFSReady Whether LittleFS is already initialized
     * @return true if successful, false otherwise
     */
    bool initialize(bool littleFSReady = false);

    /**
     * @brief Stop the web server
     */
    void stop();

    /**
     * @brief Check if the server is running
     * @return true if running, false otherwise
     */
    bool isRunning() const noexcept {
        return isRunning_;
    }

    /**
     * @brief Get the server port
     * @return Server port number
     */
    uint16_t getPort() const noexcept {
        return port_;
    }

    /**
     * @brief Send temperature event to connected clients
     * @param currentTemp Current temperature
     * @param targetTemp Target temperature
     * @param heaterPower Heater power percentage
     */
    void sendTempEvent(double currentTemp, double targetTemp, double heaterPower);

    /**
     * @brief Send weight event via WebSocket
     */
    void sendWeightEvent();

    /**
     * @brief Send generic event to connected clients
     * @param event Event name
     * @param data Event data
     */
    void sendEvent(const String& event, const String& data);

    /**
     * @brief Set system context for state access
     * @param context Pointer to SystemContext
     */
    void setSystemContext(CleverCoffee::SystemContext* context) noexcept {
        systemContext_ = context;
    }

  private:
    /**
     * @brief Setup API routes for the web server
     */
    void setupApiRoutes();

    /**
     * @brief Setup static file serving routes
     */
    void setupStaticRoutes();

    /**
     * @brief Setup middleware (CORS, Authentication)
     */
    void setupMiddleware();

    /**
     * @brief Setup event source for real-time updates
     */
    void setupEventSource();

    /**
     * @brief Handle not found requests
     */
    void handleNotFound(AsyncWebServerRequest* request);

#if !FRONTEND_PREPROCESSING
    /**
     * @brief Serve gzipped files with fallback to uncompressed
     */
    bool serveGzippedFile(AsyncWebServerRequest* request, const String& path);
#endif

#if FRONTEND_PREPROCESSING
    /**
     * @brief Template processor for static files
     */
    static String templateProcessor(const String& var);

    /**
     * @brief Static file processor for frontend preprocessing
     */
    static String staticProcessor(const String& var);

    /**
     * @brief Get header content for preprocessing
     */
    static String getHeader(const String& varName);

    /**
     * @brief Get parameter value for preprocessing
     */
    static String getValue(const String& varName);
#endif

    /**
     * @brief Get temperature data as JSON string
     */
    String getTempString() const;

    /**
     * @brief Get weight data as JSON string
     */
    String getWeightJsonString() const;

    // Web server components
    std::unique_ptr<AsyncWebServer>                server_;
    std::unique_ptr<AsyncEventSource>              events_;
    std::unique_ptr<AsyncCorsMiddleware>           corsMiddleware_;
    std::unique_ptr<AsyncAuthenticationMiddleware> authMiddleware_;

    // System context for state management
    CleverCoffee::SystemContext* systemContext_{nullptr};

    // Server state
    uint16_t port_;
    bool     isRunning_;
    bool     littleFSAvailable_;
};
