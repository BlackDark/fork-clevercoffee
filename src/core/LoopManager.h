/**
 * @file LoopManager.h
 * @brief Central manager for all main loop operations
 */

#pragma once

#include <memory>
#include <functional>
#include "../utils/Timer.h"

// Forward declarations
class ProcessController;
class SensorManager;
class UIManager;

/**
 * @class LoopManager
 * @brief Central coordinator for all main loop operations
 *
 * This class encapsulates all the components that need to be updated
 * in the main loop, providing a clean interface and proper coordination
 * between different subsystems.
 *
 * Key responsibilities:
 * - Coordinate main loop updates for all subsystems
 * - Manage timing-sensitive operations 
 * - Handle LED status updates based on machine state
 * - Coordinate water tank monitoring
 * - Manage debug timing and performance monitoring
 * - Provide clean separation between loop coordination and business logic
 */
class LoopManager {
public:
    /**
     * @brief Constructor
     * @param processController Process control manager (optional)
     * @param sensorManager Sensor management system (optional)  
     * @param uiManager UI management system (optional)
     */
    explicit LoopManager(ProcessController* processController = nullptr,
                        SensorManager* sensorManager = nullptr,
                        UIManager* uiManager = nullptr);

    /**
     * @brief Destructor
     */
    ~LoopManager() = default;

    // Disable copy constructor and assignment operator
    LoopManager(const LoopManager&) = delete;
    LoopManager& operator=(const LoopManager&) = delete;

    // Enable move constructor and assignment operator
    LoopManager(LoopManager&&) = default;
    LoopManager& operator=(LoopManager&&) = default;

    /**
     * @brief Initialize the loop manager
     * @return true if initialization successful
     */
    bool initialize();

    /**
     * @brief Main loop update - call this from Arduino loop()
     * 
     * Orchestrates updates for all subsystems in the correct order:
     * 1. Logger updates for remote logging connections
     * 2. Water tank sensor monitoring  
     * 3. Process control (PID/temperature) updates
     * 4. LED status updates based on machine state
     * 5. Debug timing and performance monitoring
     */
    void update();

    /**
     * @brief Update LED outputs based on current machine state
     * 
     * Controls status LED (temperature reached indicator),
     * brew LED (brewing state indicator), and steam LED (steam mode indicator).
     */
    void updateLEDs();

    /**
     * @brief Update water tank sensor monitoring
     * 
     * Checks water tank level using timer-based monitoring system.
     * Updates are performed every 200ms to avoid sensor noise.
     */
    void updateWaterTank();

    /**
     * @brief Update process control systems
     * 
     * Handles PID control, temperature monitoring, and machine state
     * management through ProcessController if available, otherwise
     * falls back to legacy implementation.
     */
    void updateProcessControl();

    /**
     * @brief Update network connections (WiFi/MQTT/OTA)
     * 
     * Handles WiFi connection management, MQTT communication,
     * and OTA (Over-The-Air) update processing.
     */
    void updateNetwork();

    /**
     * @brief Update website and data transmission
     * 
     * Sends temperature events and weight data to website endpoints
     * when timing conditions are met.
     */
    void updateWebsite();

    /**
     * @brief Update sensor readings (scale, pressure)
     * 
     * Processes scale weight readings and pressure sensor data
     * if enabled in configuration.
     */
    void updateSensors();

    /**
     * @brief Update switches and standby management
     * 
     * Handles steam switch, power switch checking and standby timer updates.
     */
    void updateSwitchesAndStandby();

    /**
     * @brief Update state machine
     * 
     * Processes state machine transitions and updates compatibility variables.
     */
    void updateStateMachine();

    /**
     * @brief Update display rendering and buffer management
     * 
     * Handles display template rendering, buffer management, and screen refresh.
     * This is the critical function that was missing and caused display freeze.
     * Manages both UIManager-based and fallback display logic.
     */
    void updateDisplay();

    /**
     * @brief Perform debug timing analysis
     * 
     * Monitors loop timing and performance, logging slow operations
     * and activity patterns when debug logging is enabled.
     */
    void updateDebugTiming();

    /**
     * @brief Set the process controller
     * @param controller Process controller instance
     */
    void setProcessController(ProcessController* controller) {
        processController_ = controller;
    }

    /**
     * @brief Set the sensor manager
     * @param manager Sensor manager instance
     */
    void setSensorManager(SensorManager* manager) {
        sensorManager_ = manager;
    }

    /**
     * @brief Set the UI manager
     * @param manager UI manager instance
     */
    void setUIManager(UIManager* manager) {
        uiManager_ = manager;
    }

    /**
     * @brief Get loop performance statistics
     * @return true if performance data is available
     */
    bool getPerformanceStats() const;

private:
    /**
     * @brief Setup water tank monitoring timer
     * @return true if setup successful
     */
    bool setupWaterTankTimer();

    /**
     * @brief Water tank check callback function
     */
    void checkWaterTankLevel();

    // Manager dependencies
    ProcessController* processController_;
    SensorManager* sensorManager_;
    UIManager* uiManager_;

    // Initialization state
    bool initialized_;

    // Water tank monitoring
    std::unique_ptr<Timer> waterTankTimer_;
    bool waterTankTimerInitialized_;

    // Performance monitoring
    bool performanceMonitoringEnabled_;
    unsigned long lastLoopTime_;
    unsigned long maxLoopTime_;
    unsigned long loopCount_;
};