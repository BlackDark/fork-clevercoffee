/**
 * @file LoopManager.h
 * @brief Central manager for all main loop operations
 */

#pragma once

#include "clevercoffee/utils/ModernTimer.h"

#include <functional>
#include <memory>

// Forward declarations
class ProcessController;
class UIManager;
class HotWaterHandler;

namespace CleverCoffee {
class SensorCoordinator;
class HardwareManager;
class SystemContext;
}

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
       * @param uiManager UI management system (optional)
       * @param hotWaterHandler Hot water handler (optional)
       * @param sensorCoordinator Sensor coordinator for async sensor polling (optional)
       * @param hardwareManager Hardware manager for LED and relay control (optional)
       * @param systemContext System context for access to coordinators (optional)
       */
     explicit LoopManager(ProcessController*                processController   = nullptr,
                          UIManager*                        uiManager           = nullptr,
                          HotWaterHandler*                  hotWaterHandler     = nullptr,
                          CleverCoffee::SensorCoordinator*  sensorCoordinator   = nullptr,
                          CleverCoffee::HardwareManager*    hardwareManager     = nullptr,
                          CleverCoffee::SystemContext*      systemContext       = nullptr);

    /**
     * @brief Destructor
     */
    ~LoopManager() = default;

    // Disable copy constructor and assignment operator
    LoopManager(const LoopManager&)            = delete;
    LoopManager& operator=(const LoopManager&) = delete;

    // Enable move constructor and assignment operator
    LoopManager(LoopManager&&)            = default;
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
      * @brief Set the UI manager
      * @param manager UI manager instance
      */
    void setUIManager(UIManager* manager) {
        uiManager_ = manager;
    }

    /**
     * @brief Set the hot water handler
     * @param handler Hot water handler instance
     */
    void setHotWaterHandler(HotWaterHandler* handler) {
        hotWaterHandler_ = handler;
    }

     /**
      * @brief Set the sensor coordinator
      * @param coordinator Sensor coordinator instance
      */
     void setSensorCoordinator(CleverCoffee::SensorCoordinator* coordinator) {
         sensorCoordinator_ = coordinator;
     }

      /**
       * @brief Set the hardware manager
       * @param manager Hardware manager instance
       */
      void setHardwareManager(CleverCoffee::HardwareManager* manager) {
          hardwareManager_ = manager;
      }

      /**
       * @brief Set the system context
       * @param context System context instance for coordinator access
       */
      void setSystemContext(CleverCoffee::SystemContext* context) {
          systemContext_ = context;
      }

    /**
     * @brief Get loop performance statistics
     * @return true if performance data is available
     */
    bool getPerformanceStats() const;

    /**
     * @brief Configure sensor update timers with specific intervals
     * @param temperatureIntervalMs Temperature sensor update interval in milliseconds
     * @param pressureIntervalMs Pressure sensor update interval in milliseconds
     * @param scaleIntervalMs Scale sensor update interval in milliseconds
     */
    void configureSensorTimers(unsigned long temperatureIntervalMs = 400,   // 2.5 Hz
                               unsigned long pressureIntervalMs = 50,       // 20 Hz
                               unsigned long scaleIntervalMs = 100);        // 10 Hz

    /**
     * @brief Log configured timer intervals and actual execution frequencies
     */
    void logTimerConfiguration() const;

  private:
    /**
     * @brief Setup all timers (sensors, water tank, general)
     * @return true if setup successful
     */
    bool setupAllTimers();

    /**
     * @brief Sensor timer callback functions
     */
    void updateTemperatureSensor();
    void updatePressureSensor();
    void updateScaleSensor();
    void updateBrewWeight();
    void checkWaterTankLevel();

    /**
     * @brief Invoke all centralized sensor timers
     */
    void updateCentralizedSensorTimers();

      // Manager dependencies
      ProcessController*               processController_;
      UIManager*                       uiManager_;
      HotWaterHandler*                 hotWaterHandler_;
      CleverCoffee::SensorCoordinator* sensorCoordinator_;
      CleverCoffee::HardwareManager*   hardwareManager_;
      CleverCoffee::SystemContext*     systemContext_;

    // Initialization state
    bool initialized_;

    // Centralized timer system for all sensors
    std::unique_ptr<MillisecondTimer> waterTankTimer_;
    std::unique_ptr<MillisecondTimer> temperatureTimer_;
    std::unique_ptr<MillisecondTimer> pressureTimer_;
    std::unique_ptr<MillisecondTimer> scaleTimer_;
    bool                              sensorsTimersInitialized_;

    // Performance monitoring
    bool          performanceMonitoringEnabled_;
    unsigned long lastLoopTime_;
    unsigned long maxLoopTime_;
    unsigned long loopCount_;

    // Timer execution counters for monitoring
    unsigned long temperatureUpdateCount_;
    unsigned long pressureUpdateCount_;
    unsigned long scaleUpdateCount_;
    unsigned long lastTimerLogTime_;
};
