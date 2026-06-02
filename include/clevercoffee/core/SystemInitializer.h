/**
 * @file SystemInitializer.h
 * @brief RAII wrapper for system initialization
 */

#pragma once

#include "clevercoffee/Logger.h"

#include <Arduino.h>
#include <functional>
#include <memory>

// Forward declarations
class DisplayManager;
class OledDriver;
class MQTTManager;
class WebServerManager;
class CleverCoffeeWiFiManager;
class PID;
class BrewHandler;
class HotWaterHandler;
class PowerHandler;
class SteamHandler;

namespace CleverCoffee {
class HardwareManager;
class SystemContext;
} // namespace CleverCoffee

/**
 * @enum InitState
 * @brief System initialization state
 */
enum class InitState {
    NOT_INITIALIZED, ///< System not yet initialized
    INITIALIZING,    ///< System initialization in progress
    INITIALIZED,     ///< System fully initialized and ready
    FAILED           ///< System initialization failed
};

/**
 * @class SystemInitializer
 * @brief RAII wrapper for complete system initialization
 *
 * This class provides safe management of system initialization using RAII principles.
 * It encapsulates all initialization phases with proper error handling and logging.
 *
 * Component Criticality:
 * - CRITICAL: HardwareManager, SystemContext, ProcessController (must exist, system exits if fails)
 * - IMPORTANT: DisplayManager, NetworkManager (should exist, system degrades if fails)
 * - OPTIONAL: MQTTManager, some LEDs (may or may not exist)
 */
class SystemInitializer {
  public:
    /**
     * @brief Constructor - initializes system initializer
     */
    SystemInitializer();

    /**
     * @brief Destructor - automatically cleans up resources
     */
    ~SystemInitializer();

    // Disable copy constructor and assignment operator
    SystemInitializer(const SystemInitializer&)            = delete;
    SystemInitializer& operator=(const SystemInitializer&) = delete;

    // Enable move constructor and assignment operator
    SystemInitializer(SystemInitializer&&)            = default;
    SystemInitializer& operator=(SystemInitializer&&) = default;

    /**
     * @brief Initialize complete system
     * @return true if initialization successful
     */
    [[nodiscard]] bool initialize();

    /**
     * @brief Check if system is initialized
     * @return true if system is ready
     */
    [[nodiscard]] bool isInitialized() const {
        return systemInitialized_;
    }

    /**
     * @brief Get initialization state
     * @return Current initialization state
     */
    InitState getInitState() const {
        return initState_;
    }

    /**
     * @brief Get display manager
     * @return Pointer to display manager (may be null)
     */
    DisplayManager& getDisplayManager() const {
        if (!displayManager_) {
            LOG(FATAL, "DisplayManager not initialized - system bug!");
        }
        return *displayManager_;
    }

    /**
     * @brief Get UI manager (REQUIRED)
     * @return Reference to UI manager
     */
    OledDriver& getOledDriver() const {
        if (!oledDriver_) {
            LOG(FATAL, "OledDriver not initialized - system bug!");
        }
        return *oledDriver_;
    }

    /**
     * @brief Get hardware manager (CRITICAL component)
     * @return Reference to hardware manager
     * @throws std::logic_error if hardware manager not initialized or system not ready
     */
    CleverCoffee::HardwareManager& getHardwareManager() const {
        if (initState_ != InitState::INITIALIZED) {
            LOG(FATAL, "System not initialized - cannot access HardwareManager");
            // In embedded systems, we can't throw, so use emergency shutdown
            // For now, log fatal error - caller should check isInitialized() first
        }
        if (!hardwareManager_) {
            LOG(FATAL, "HardwareManager not initialized - system bug!");
            // This should never happen if initialization succeeded
        }
        return *hardwareManager_;
    }

    /**
     * @brief Get MQTT manager (REQUIRED)
     * @return Reference to MQTT manager
     */
    MQTTManager& getMQTTManager() const {
        if (!mqttManager_) {
            LOG(FATAL, "MQTTManager not initialized - system bug!");
        }
        return *mqttManager_;
    }

    /**
     * @brief Get WiFi manager (REQUIRED)
     * @return Reference to WiFi manager
     */
    CleverCoffeeWiFiManager& getWiFiManager() const;

    /**
     * @brief Get web server manager
     * @return Pointer to web server manager (may be null)
     */
    WebServerManager* getWebServerManager() const {
        return webServerManager_.get();
    }

    /**
     * @brief Get system context (CRITICAL component)
     * @return Reference to system context
     * @throws std::logic_error if system context not initialized or system not ready
     */
    CleverCoffee::SystemContext& getSystemContext() const {
        if (initState_ != InitState::INITIALIZED) {
            LOG(FATAL, "System not initialized - cannot access SystemContext");
        }
        if (!systemContext_) {
            LOG(FATAL, "SystemContext not initialized - system bug!");
        }
        return *systemContext_;
    }

    /**
     * @brief Finalize machine state - must be called AFTER StateMachine is created and registered
     * @note This is called from main.cpp after MachineStateContext is available
     * @return True if successful, false otherwise
     */
    [[nodiscard]] bool finalizeMachineState();

  private:
    // Initialization state
    bool      systemInitialized_;
    InitState initState_;
    String    hostname_;

    // Manager instances
    std::unique_ptr<DisplayManager>                displayManager_;
    std::unique_ptr<OledDriver>                    oledDriver_;
    std::unique_ptr<CleverCoffee::HardwareManager> hardwareManager_;
    std::unique_ptr<MQTTManager>                   mqttManager_;
    std::unique_ptr<CleverCoffeeWiFiManager>       cleverCoffeeWiFiManager_;
    std::unique_ptr<WebServerManager>              webServerManager_;
    std::unique_ptr<PID>                           pidController_;
    std::unique_ptr<CleverCoffee::SystemContext>   systemContext_;

    // Handler instances (owned here, registered as non-owning pointers in SystemContext)
    std::unique_ptr<BrewHandler>     brewHandler_;
    std::unique_ptr<HotWaterHandler> hotWaterHandler_;
    std::unique_ptr<PowerHandler>    powerHandler_;
    std::unique_ptr<SteamHandler>    steamHandler_;

    // Initialization phases
    bool initializeLogger();
    bool initializeConfiguration();
    bool initializeDisplay();
    bool initializeHardware();
    bool initializeHandlers();
    bool initializeNetworking();
    bool initializeMQTT();
    bool initializePID();
    bool initializeSensors();

    // Network setup helpers
    void setupWiFi();

    // Helper methods
    void calculateDerivedValues();
    void setupTiming();
    void registerMQTTParameters();
    void registerMQTTSensors();
};
