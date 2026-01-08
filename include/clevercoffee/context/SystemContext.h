#pragma once

#include "clevercoffee/context/HardwareContext.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/coordinators/NetworkCoordinator.h"
#include "clevercoffee/coordinators/UICoordinator.h"
#include "clevercoffee/coordinators/StandbyCoordinator.h"

// Forward declarations for handlers
class BrewHandler;
class HotWaterHandler;
class PowerHandler;
class SteamHandler;
class ProcessController;
class MachineStateContext;
class MQTTManager;
class CleverCoffeeWiFiManager;
class WebServerManager;

namespace CleverCoffee {

/**
 * @brief Central context for system-wide shared state
 *
 * This class serves as the central hub for all system-wide coordination state,
 * replacing the previous global g_state variable with explicit dependency injection.
 * It manages all coordinators that handle cross-cutting concerns like sensor updates,
 * network operations, and UI refreshes.
 *
 * Design Pattern: Dependency Injection / Service Locator
 * - All components that need access to system state receive a reference to SystemContext
 * - This makes dependencies explicit and improves testability
 * - Thread-safe access is provided through atomic operations in coordinators
 *
 * Lifecycle:
 * 1. Create SystemContext instance during system initialization
 * 2. Pass references to components that need access
 * 3. Call markReady() when all initialization is complete
 * 4. Use isReady() to guard operations that require full initialization
 *
 * Example usage:
 * @code
 * // During initialization
 * SystemContext systemContext;
 *
 * // Inject into components
 * SensorManager sensorMgr(systemContext.sensorCoordinator());
 * NetworkManager netMgr(systemContext.networkCoordinator());
 *
 * // Check initialization state
 * void performOperation(SystemContext& ctx) {
 *     if (!ctx.isReady()) {
 *         return ErrorCode::NOT_INITIALIZED;
 *     }
 *     // ... perform operation ...
 * }
 *
 * // Access coordinators
 * if (ctx.sensorCoordinator().isTemperatureUpdateRunning()) {
 *     // Handle concurrent access
 * }
 * @endcode
 */
class SystemContext {
public:
    SystemContext() = default;

    /**
     * @name Coordinator Access
     * @{
     */

    /**
     * @brief Access sensor coordinator
     *
     * Provides access to the SensorCoordinator for managing sensor operations.
     *
     * @return Reference to the sensor coordinator
     */
    SensorCoordinator& sensorCoordinator() noexcept { return sensorCoordinator_; }

    /**
     * @brief Access sensor coordinator (const overload)
     *
     * @return Const reference to the sensor coordinator
     */
    const SensorCoordinator& sensorCoordinator() const noexcept { return sensorCoordinator_; }

    /**
     * @brief Access network coordinator
     *
     * Provides access to the NetworkCoordinator for managing network state.
     *
     * @return Reference to the network coordinator
     */
    NetworkCoordinator& networkCoordinator() noexcept { return networkCoordinator_; }

    /**
     * @brief Access network coordinator (const overload)
     *
     * @return Const reference to the network coordinator
     */
    const NetworkCoordinator& networkCoordinator() const noexcept { return networkCoordinator_; }

    /**
     * @brief Access UI coordinator
     *
     * Provides access to the UICoordinator for managing display operations.
     *
     * @return Reference to the UI coordinator
     */
    UICoordinator& uiCoordinator() noexcept { return uiCoordinator_; }

    /**
     * @brief Access UI coordinator (const overload)
     *
     * @return Const reference to the UI coordinator
     */
    const UICoordinator& uiCoordinator() const noexcept { return uiCoordinator_; }

    /**
     * @brief Access standby coordinator
     *
     * Provides access to the StandbyCoordinator for managing standby mode.
     *
     * @return Reference to the standby coordinator
     */
    StandbyCoordinator& standbyCoordinator() noexcept { return standbyCoordinator_; }

    /**
     * @brief Access standby coordinator (const overload)
     *
     * @return Const reference to the standby coordinator
     */
    const StandbyCoordinator& standbyCoordinator() const noexcept { return standbyCoordinator_; }

    /**
     * @brief Access hardware context
     *
     * Provides access to the HardwareContext for accessing hardware components.
     *
     * @return Reference to the hardware context
     */
    HardwareContext& hardwareContext() noexcept { return hardwareContext_; }

    /**
     * @brief Access hardware context (const overload)
     *
     * @return Const reference to the hardware context
     */
    const HardwareContext& hardwareContext() const noexcept { return hardwareContext_; }

    /** @} */

    /**
     * @name Handler Registration and Access
     * @{
     */

    /**
     * @brief Register brew handler
     * @param handler Pointer to BrewHandler instance (can be nullptr)
     */
    void setBrewHandler(BrewHandler* handler) noexcept { brewHandler_ = handler; }

    /**
     * @brief Get brew handler
     * @return Pointer to BrewHandler (may be nullptr if not registered)
     */
    BrewHandler* brewHandler() noexcept { return brewHandler_; }

    /**
     * @brief Get brew handler (const)
     * @return Const pointer to BrewHandler (may be nullptr if not registered)
     */
    const BrewHandler* brewHandler() const noexcept { return brewHandler_; }

    /**
     * @brief Register hot water handler
     * @param handler Pointer to HotWaterHandler instance (can be nullptr)
     */
    void setHotWaterHandler(HotWaterHandler* handler) noexcept { hotWaterHandler_ = handler; }

    /**
     * @brief Get hot water handler
     * @return Pointer to HotWaterHandler (may be nullptr if not registered)
     */
    HotWaterHandler* hotWaterHandler() noexcept { return hotWaterHandler_; }

    /**
     * @brief Get hot water handler (const)
     * @return Const pointer to HotWaterHandler (may be nullptr if not registered)
     */
    const HotWaterHandler* hotWaterHandler() const noexcept { return hotWaterHandler_; }

    /**
     * @brief Register power handler
     * @param handler Pointer to PowerHandler instance (can be nullptr)
     */
    void setPowerHandler(PowerHandler* handler) noexcept { powerHandler_ = handler; }

    /**
     * @brief Get power handler
     * @return Pointer to PowerHandler (may be nullptr if not registered)
     */
    PowerHandler* powerHandler() noexcept { return powerHandler_; }

    /**
     * @brief Get power handler (const)
     * @return Const pointer to PowerHandler (may be nullptr if not registered)
     */
    const PowerHandler* powerHandler() const noexcept { return powerHandler_; }

    /**
     * @brief Register steam handler
     * @param handler Pointer to SteamHandler instance (can be nullptr)
     */
    void setSteamHandler(SteamHandler* handler) noexcept { steamHandler_ = handler; }

    /**
     * @brief Get steam handler
     * @return Pointer to SteamHandler (may be nullptr if not registered)
     */
    SteamHandler* steamHandler() noexcept { return steamHandler_; }

    /**
     * @brief Get steam handler (const)
     * @return Const pointer to SteamHandler (may be nullptr if not registered)
     */
    const SteamHandler* steamHandler() const noexcept { return steamHandler_; }

    /**
     * @brief Register process controller
     * @param controller Pointer to ProcessController instance (can be nullptr)
     */
    void setProcessController(ProcessController* controller) noexcept { processController_ = controller; }

    /**
     * @brief Get process controller
     * @return Pointer to ProcessController (may be nullptr if not registered)
     */
    ProcessController* processController() noexcept { return processController_; }

     /**
      * @brief Get process controller (const)
      * @return Const pointer to ProcessController (may be nullptr if not registered)
      */
     const ProcessController* processController() const noexcept { return processController_; }

     /**
      * @brief Register machine state context
      * @param context Pointer to MachineStateContext instance (can be nullptr)
      */
     void setMachineStateContext(MachineStateContext* context) noexcept { machineStateContext_ = context; }

     /**
      * @brief Get machine state context
      * @return Pointer to MachineStateContext (may be nullptr if not registered)
      */
     MachineStateContext* machineStateContext() noexcept { return machineStateContext_; }

     /**
      * @brief Get machine state context (const)
      * @return Const pointer to MachineStateContext (may be nullptr if not registered)
      */
     const MachineStateContext* machineStateContext() const noexcept { return machineStateContext_; }

     /**
      * @name Network Managers
      * @{
      */

     /**
      * @brief Register MQTT manager
      * @param manager Pointer to MQTTManager instance (can be nullptr)
      */
     void setMQTTManager(MQTTManager* manager) noexcept { mqttManager_ = manager; }

     /**
      * @brief Get MQTT manager
      * @return Pointer to MQTTManager (may be nullptr if not registered)
      */
     MQTTManager* mqttManager() noexcept { return mqttManager_; }

     /**
      * @brief Get MQTT manager (const)
      * @return Const pointer to MQTTManager (may be nullptr if not registered)
      */
     const MQTTManager* mqttManager() const noexcept { return mqttManager_; }

     /**
      * @brief Register WiFi manager
      * @param manager Pointer to CleverCoffeeWiFiManager instance (can be nullptr)
      */
     void setCleverCoffeeWiFiManager(CleverCoffeeWiFiManager* manager) noexcept { cleverCoffeeWiFiManager_ = manager; }

     /**
      * @brief Get WiFi manager
      * @return Pointer to CleverCoffeeWiFiManager (may be nullptr if not registered)
      */
     CleverCoffeeWiFiManager* cleverCoffeeWiFiManager() noexcept { return cleverCoffeeWiFiManager_; }

     /**
      * @brief Get WiFi manager (const)
      * @return Const pointer to CleverCoffeeWiFiManager (may be nullptr if not registered)
      */
     const CleverCoffeeWiFiManager* cleverCoffeeWiFiManager() const noexcept { return cleverCoffeeWiFiManager_; }

     /**
      * @brief Register WebServer manager
      * @param manager Pointer to WebServerManager instance (can be nullptr)
      */
     void setWebServerManager(WebServerManager* manager) noexcept { webServerManager_ = manager; }

     /**
      * @brief Get WebServer manager
      * @return Pointer to WebServerManager (may be nullptr if not registered)
      */
     WebServerManager* webServerManager() noexcept { return webServerManager_; }

     /**
      * @brief Get WebServer manager (const)
      * @return Const pointer to WebServerManager (may be nullptr if not registered)
      */
     const WebServerManager* webServerManager() const noexcept { return webServerManager_; }

     /** @} */


      /** @} */

      /**
       * @name Process State Accessors
       * Replaces direct g_state.process.* access with explicit methods.
       * Temporary implementations delegate to GlobalState during transition.
       * @{
       */

      /**
       * @brief Get current measured temperature
       * @return Temperature in Celsius
       */
      double processTemperature() const noexcept;

      /**
       * @brief Set current measured temperature
       * @param temp Temperature in Celsius
       */
      void setProcessTemperature(double temp) noexcept;

      /**
       * @brief Get target setpoint temperature
       * @return Temperature in Celsius
       */
      double processSetpoint() const noexcept;

      /**
       * @brief Set target setpoint temperature
       * @param setpoint Temperature in Celsius
       */
      void setProcessSetpoint(double setpoint) noexcept;

      /**
       * @brief Get current PID output value
       * @return PID output (typically 0-1000)
       */
      double processPidOutput() const noexcept;

      /**
       * @brief Set current PID output value
       * @param output PID output value
       */
      void setProcessPidOutput(double output) noexcept;

      /**
       * @brief Get elapsed brew time
       * @return Time in milliseconds
       */
      double processCurrentBrewTime() const noexcept;

      /**
       * @brief Set elapsed brew time
       * @param time Time in milliseconds
       */
      void setProcessCurrentBrewTime(double time) noexcept;

      /**
       * @brief Get target brew time
       * @return Time in milliseconds
       */
      double processTotalTargetBrewTime() const noexcept;

      /**
       * @brief Set target brew time
       * @param time Time in milliseconds
       */
      void setProcessTotalTargetBrewTime(double time) noexcept;

      /**
       * @brief Check if brew PID is disabled
       * @return true if disabled, false otherwise
       */
      bool isProcessBrewPidDisabled() const noexcept;

      /**
       * @brief Set brew PID disabled state
       * @param disabled true to disable, false to enable
       */
      void setProcessBrewPidDisabled(bool disabled) noexcept;

      /**
       * @brief Get previous input value (for PID derivative)
       * @return Previous temperature reading
       */
      double processPreviousInput() const noexcept;

      /**
       * @brief Set previous input value
       * @param input Previous temperature reading
       */
      void setProcessPreviousInput(double input) noexcept;

      /**
       * @brief Get PID aggressive Ki parameter
       * @return Integral gain for aggressive mode
       */
      double processPidAggKi() const noexcept;

      /**
       * @brief Set PID aggressive Ki parameter
       * @param value Integral gain for aggressive mode
       */
      void setProcessPidAggKi(double value) noexcept;

      /**
       * @brief Get PID aggressive Kd parameter
       * @return Derivative gain for aggressive mode
       */
      double processPidAggKd() const noexcept;

      /**
       * @brief Set PID aggressive Kd parameter
       * @param value Derivative gain for aggressive mode
       */
      void setProcessPidAggKd(double value) noexcept;

      /**
       * @brief Get PID normal Ki parameter
       * @return Integral gain for normal mode
       */
      double processPidKi() const noexcept;

      /**
       * @brief Set PID normal Ki parameter
       * @param value Integral gain for normal mode
       */
      void setProcessPidKi(double value) noexcept;

      /**
       * @brief Get PID normal Kd parameter
       * @return Derivative gain for normal mode
       */
      double processPidKd() const noexcept;

       /**
        * @brief Set PID normal Kd parameter
        * @param value Derivative gain for normal mode
        */
       void setProcessPidKd(double value) noexcept;

       /**
        * @brief Get PID window size
        * @return Window size in milliseconds
        */
       int processWindowSize() const noexcept;

       /**
        * @brief Set PID window size
        * @param size Window size in milliseconds
        */
       void setProcessWindowSize(int size) noexcept;

       /**
        * @brief Check if PID is enabled
        * @return True if PID is enabled
        */
       bool isProcessPidEnabled() const noexcept;

       /**
        * @brief Set PID enabled state
        * @param enabled True to enable PID
        */
       void setProcessPidEnabled(bool enabled) noexcept;

       /** @} */

      /**
       * @name Display Data Snapshot
       * Provides atomic read of all display-related data
       * @{
       */

      /**
       * @brief Immutable snapshot of display data
       *
       * Contains all data needed for rendering display at a point in time.
       * Using snapshots provides atomic reads and thread-safety guarantees.
       */
      struct DisplaySnapshot {
          // Process data
          double currentTemperature = 0.0;    ///< Current measured temperature (°C)
          double setpointTemperature = 0.0;   ///< Target setpoint (°C)
          double pidOutputPercent = 0.0;      ///< Heater power (0-1000)
          double currentBrewTime = 0.0;       ///< Elapsed brew time (ms)
          double targetBrewTime = 0.0;        ///< Target brew duration (ms)
          bool brewPidDisabled = false;       ///< Brew PID active state

          // PID tuning values
          double pidKp = 0.0;                 ///< Proportional gain
          double pidKi = 0.0;                 ///< Integral gain
          double pidKd = 0.0;                 ///< Derivative gain

          // Sensor data
          double pumpOnTime = 0.0;            ///< Hot water pump runtime (ms)
          float inputPressure = 0.0f;         ///< Current pump pressure (bar)
          double brewWeight = 0.0;            ///< Current shot weight (grams)

          // Timing/animation
          unsigned int isrCounter = 0;        ///< ISR counter for animation sync

          // Coordination flags
          bool displayBufferReady = false;    ///< Display update ready flag
      };

      /**
       * @brief Get atomic snapshot of all display data
       * @return DisplaySnapshot containing current display state
       */
      DisplaySnapshot getDisplaySnapshot() const noexcept;

      /**
       * @brief Mark display buffer as ready for rendering
       * @param ready true if buffer is ready, false otherwise
       */
      void markDisplayBufferReady(bool ready) noexcept;

      /** @} */

      /**
       * @name Command/Control Accessors
       * Replaces direct g_state writes from web/MQTT endpoints
       * @{
       */

      /**
       * @brief Request scale tare operation
       * Signals sensor coordinator to tare (zero) the scale
       */
      void requestScaleTare() noexcept;

      /**
       * @brief Request scale calibration operation
       * Signals sensor coordinator to start calibration procedure
       */
      void requestScaleCalibration() noexcept;

      /**
       * @brief Notify that Home Assistant discovery is running
       * @param running true if discovery is active, false otherwise
       */
      void setHassioDiscoveryRunning(bool running) noexcept;

      /**
       * @brief Notify that Home Assistant connection failed
       * @param failed true if connection failed, false otherwise
       */
      void setHassioFailed(bool failed) noexcept;

      /** @} */

      /**
       * @name Utility Accessors
       * @{
       */

      /**
       * @brief Update pressure filter with new reading
       * @param input Raw pressure reading
       */
      void updatePressureFilter(float input) noexcept;

      /**
       * @brief Get filtered pressure output
       * @return Filtered pressure value
       */
      float getPressureFilterOutput() const noexcept;

      /** @} */

     /**
      * @name System Initialization State
      * @{
      */

    /**
     * @brief Mark the system as fully initialized
     *
     * Should be called once all system components are successfully initialized.
     * Components can check isReady() before performing operations.
     *
     * @post isReady() returns true
     */
    void markReady() noexcept { ready_ = true; }

    /**
     * @brief Check if system is fully initialized
     *
     * @return true if the system has completed initialization, false otherwise
     */
    bool isReady() const noexcept { return ready_; }

    /** @} */

private:
    HardwareContext hardwareContext_;         ///< Hardware component registry
    SensorCoordinator sensorCoordinator_;     ///< Manages sensor operation state
    NetworkCoordinator networkCoordinator_;   ///< Manages network connection state
    UICoordinator uiCoordinator_;             ///< Manages UI refresh and sleep state
    StandbyCoordinator standbyCoordinator_;   ///< Manages standby mode and power management
    bool ready_ = false;                      ///< System initialization complete flag

    // Handler references (non-owning pointers)
    BrewHandler*     brewHandler_     = nullptr;
    HotWaterHandler* hotWaterHandler_ = nullptr;
    PowerHandler*    powerHandler_    = nullptr;
    SteamHandler*    steamHandler_    = nullptr;
    
     // Controller reference (non-owning pointer)
     ProcessController* processController_ = nullptr;

     // Machine state context reference (non-owning pointer)
     MachineStateContext* machineStateContext_ = nullptr;

     // Manager references (non-owning pointers)
     MQTTManager* mqttManager_ = nullptr;
     CleverCoffeeWiFiManager* cleverCoffeeWiFiManager_ = nullptr;
     WebServerManager* webServerManager_ = nullptr;
};

/**
 * @brief Global system context accessor
 * 
 * Returns the global SystemContext instance if initialized.
 * Used by utility functions and inline code that need system context access.
 * 
 * @warning Returns nullptr if context is not initialized
 * @return Pointer to the global SystemContext instance, or nullptr
 */
extern SystemContext* g_systemContext;

/**
 * @brief Set the global system context reference
 * 
 * Called during system initialization to register the SystemContext.
 * 
 * @param context Pointer to the SystemContext instance
 */
inline void setGlobalSystemContext(SystemContext* context) {
    g_systemContext = context;
}

/**
 * @brief Get the global system context reference
 * 
 * @return Pointer to the global SystemContext instance, or nullptr if not initialized
 */
inline SystemContext* getGlobalSystemContext() {
    return g_systemContext;
}

} // namespace CleverCoffee
