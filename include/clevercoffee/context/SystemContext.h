#pragma once

#include "clevercoffee/context/HardwareContext.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/coordinators/NetworkCoordinator.h"
#include "clevercoffee/coordinators/UICoordinator.h"
#include "clevercoffee/coordinators/StandbyCoordinator.h"
#include "clevercoffee/types/GlobalTypes.h"  // For type definitions (cmp_str, MachineStateFlags, etc.)
#include "clevercoffee/utils/ModernTimer.h"  // For MillisecondTimer

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
class PID;

namespace CleverCoffee {

/**
 * @brief Central context for system-wide shared state
 *
 * This class serves as the central hub for all system-wide coordination state.
 * All state is encapsulated as private members (previously in a global variable).
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
       * Centralized access to process state through SystemContext.
       * Implementations delegate to GlobalState for unified state management.
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

       /**
        * @brief Get pointer to temperature value for PID controller
        * @return Pointer to process temperature value
        */
       double* processTemperaturePtr() noexcept;

       /**
        * @brief Get pointer to PID output value
        * @return Pointer to process PID output value
        */
       double* processPidOutputPtr() noexcept;

       /**
        * @brief Get pointer to setpoint value for PID controller
        * @return Pointer to process setpoint value
        */
       double* processSetpointPtr() noexcept;

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
        * Control methods for web/MQTT endpoints and handlers
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
       * @name Critical Machine Control Accessors
       * Timer, Emergency Stop, and PID operations
       * @{
       */

      /**
       * @brief Get the hardware timer for ISR management
       * @return Pointer to timer_t object, or nullptr if not initialized
       */
      hw_timer_t* machineTimer() noexcept;

      /**
       * @brief Set the hardware timer for ISR management
       * @param timer Pointer to timer object
       */
      void setMachineTimer(hw_timer_t* timer) noexcept;

      /**
       * @brief Check if the machine timer is initialized
       * @return true if timer is initialized, false otherwise
       */
      bool isMachineTimerInitialized() const noexcept;

      /**
       * @brief Get the ISR counter for animation timing
       * @return Current ISR counter value
       */
      unsigned int isrCounter() const noexcept;

      /**
       * @brief Set the ISR counter
       * @param value New counter value
       */
      void setIsrCounter(unsigned int value) noexcept;

      /**
       * @brief Increment the ISR counter
       * Used in interrupt handlers for animation timing
       */
      void incrementIsrCounter() noexcept;

      /**
       * @brief Check if emergency stop is active
       * Safety-critical flag for temperature overrun protection
       * @return true if emergency stop is active
       */
      bool isEmergencyStopActive() const noexcept;

      /**
       * @brief Set emergency stop state
       * @param active true to activate, false to deactivate
       */
      void setEmergencyStop(bool active) noexcept;

      /**
       * @brief Trigger emergency stop
       * Centralized method for all emergency stop requests
       */
      void triggerEmergencyStop() noexcept;

      /**
       * @brief Compute PID controller
       * Updates PID output based on current temperature and setpoint
       */
      void computePid() noexcept;

      /**
       * @brief Set PID tuning parameters
       * @param kp Proportional gain
       * @param ki Integral gain
       * @param kd Derivative gain
       * @param ponM Control on measurement (1) or error (0)
       */
      void setPidTunings(double kp, double ki, double kd, int ponM = 1) noexcept;

      /**
       * @brief Set PID operating mode
       * @param mode AUTOMATIC or MANUAL
       */
      void setPidMode(int mode) noexcept;

      /**
       * @brief Set PID output limits
       * @param min Minimum output value
       * @param max Maximum output value
       */
      void setPidOutputLimits(double min, double max) noexcept;

      /**
       * @brief Set PID integrator limits
       * @param min Minimum integrator value
       * @param max Maximum integrator value
       */
      void setPidIntegratorLimits(double min, double max) noexcept;

      /**
       * @brief Set PID sample time
       * @param sampleTime Sample time in milliseconds
       */
      void setPidSampleTime(int sampleTime) noexcept;

      /**
       * @brief Set PID smoothing factor (EMA)
       * @param factor Smoothing factor (0-1)
       */
      void setPidSmoothingFactor(double factor) noexcept;

      /**
       * @brief Get current PID mode
       * @return AUTOMATIC or MANUAL
       */
      int pidMode() const noexcept;

      /**
       * @brief Get PID proportional gain
       * @return Current Kp value
       */
      double pidKp() const noexcept;

      /**
       * @brief Get PID integral gain
       * @return Current Ki value
       */
      double pidKi() const noexcept;

      /**
       * @brief Get PID derivative gain
       * @return Current Kd value
       */
      double pidKd() const noexcept;

      /**
       * @brief Get last proportional term
       * @return P component of last PID output
       */
      double pidLastPPart() const noexcept;

      /**
       * @brief Get last integral term
       * @return I component of last PID output
       */
      double pidLastIPart() const noexcept;

      /**
       * @brief Get last derivative term
       * @return D component of last PID output
       */
      double pidLastDPart() const noexcept;

      /**
       * @brief Get PID input error
       * @return Current error (setpoint - input)
       */
      double pidInputError() const noexcept;

      /**
       * @brief Get PID delta input (derivative term input)
       * @return Rate of change of input
       */
      double pidDeltaInput() const noexcept;

      /**
       * @brief Set the PID controller instance
       * @param pid Pointer to PID controller instance
       */
      void setPidController(PID* pid) noexcept;

      /**
       * @brief Direct access to PID controller
       * For backward compatibility during migration
       * @return Pointer to PID object
       */
      PID* pidController() noexcept;

      /**
       * @brief Direct access to PID controller (const)
       * @return Const pointer to PID object
       */
      const PID* pidController() const noexcept;

       /** @} */

       /**
        * @name Scale and Sensor Operations
        * @{
        */

       /**
        * @brief Check if scale calibration is currently active
        * @return true if scale calibration mode is on
        */
       bool scaleCalibrationOn() const noexcept;

       /**
        * @brief Set scale calibration active state
        * @param on true to activate calibration, false to deactivate
        */
       void setScaleCalibrationOn(bool on) noexcept;

       /**
        * @brief Check if scale tare operation is active
        * @return true if scale tare is in progress
        */
       bool scaleTareOn() const noexcept;

       /**
        * @brief Set scale tare active state
        * @param on true to activate tare, false to deactivate
        */
       void setScaleTareOn(bool on) noexcept;

       /**
        * @brief Get current brew weight reading
        * @return Current weight in grams
        */
       double currBrewWeight() const noexcept;

       /**
        * @brief Set current brew weight
        * @param weight Weight in grams
        */
       void setCurrBrewWeight(double weight) noexcept;

       /**
        * @brief Get current scale reading weight
        * @return Current weight reading in grams
        */
       double currReadingWeight() const noexcept;

       /**
        * @brief Set current scale reading weight
        * @param weight Weight in grams
        */
       void setCurrReadingWeight(double weight) noexcept;

       /**
        * @brief Get current pump on time
        * @return Pump on time in seconds
        */
       double currPumpOnTime() const noexcept;

       /**
        * @brief Set current pump on time
        * @param time Pump on time in seconds
        */
       void setCurrPumpOnTime(double time) noexcept;

       /**
        * @brief Get input pressure reading
        * @return Pressure value
        */
       float inputPressure() const noexcept;

       /**
        * @brief Set input pressure value
        * @param pressure Pressure value
        */
       void setInputPressure(float pressure) noexcept;

       /**
        * @brief Check if scale has encountered a failure
        * @return true if scale failure is detected
        */
       bool scaleFailure() const noexcept;

       /**
        * @brief Set scale failure state
        * @param failed true if scale failed, false if operational
        */
       void setScaleFailure(bool failed) noexcept;

       /** @} */

       /**
        * @name Network Manager References
        * @{
        */

       /**
        * @brief Get WiFi manager reference
        * @return Pointer to WiFi manager
        */
       CleverCoffeeWiFiManager* wifiManager() noexcept;

       /**
        * @brief Set WiFi manager reference
        * @param manager Pointer to WiFi manager
        */
       void setWifiManager(CleverCoffeeWiFiManager* manager) noexcept;

       /**
        * @brief Check if system is in offline mode
        * @return true if offline mode is active
        */
       bool offlineMode() const noexcept;

       /**
        * @brief Set offline mode state
        * @param offline true to enable offline mode
        */
       void setOfflineMode(bool offline) noexcept;

       /**
        * @brief Check if Hass.io discovery is currently running
        * @return true if discovery is running
        */
       bool hassioDiscoveryRunning() const noexcept;

       /**
        */
       bool hassioFailed() const noexcept;

       /**
        */
       unsigned int wifiReconnects() const noexcept;

       /**
        * @brief Set WiFi reconnect attempt count
        * @param count Number of reconnect attempts
        */
       void setWifiReconnects(unsigned int count) noexcept;

       /** @} */

       /**
        * @name Machine Mode Flags
        * @{
        */

       /**
        * @brief Check if steam mode is active
        * @return true if steam mode is enabled
        */
       bool steamMode() const noexcept;

       /**
        * @brief Set steam mode
        * @param on true to enable steam mode
        */
       void setSteamMode(bool on) noexcept;

       /**
        * @brief Check if steam first mode is on
        * @return true if steam first mode is active
        */
       bool steamFirstOn() const noexcept;

       /**
        * @brief Set steam first mode
        * @param on true to enable steam first mode
        */
       void setSteamFirstOn(bool on) noexcept;

       /**
        * @brief Check if backflush mode is active
        * @return true if backflush mode is enabled
        */
       bool backflushMode() const noexcept;

       /**
        * @brief Set backflush mode
        * @param on true to enable backflush mode
        */
       void setBackflushMode(bool on) noexcept;

        /** @} */

        /**
         * @name Display Coordination
         * @{
         */

        /**
         * @brief Check if display buffer is ready for rendering
         * @return true if display update buffer is ready
         */
        bool displayBufferReady() const noexcept;

        /**
         * @brief Set display buffer ready state
         * @param ready true if buffer is ready
         */
        void setDisplayBufferReady(bool ready) noexcept;

        /** @} */

        /**
         * @name Pressure Filter Variables
         * Exponential Moving Average filter for pressure sensor readings
         * @{
         */

        /**
         * @brief Get pressure filter X input value
         * @return Current X (weighted input) value
         */
        float inX() const noexcept;

        /**
         * @brief Set pressure filter X input value
         * @param value X value to set
         */
        void setInX(float value) noexcept;

        /**
         * @brief Get pressure filter Y output value
         * @return Current Y (filtered output) value
         */
        float inY() const noexcept;

        /**
         * @brief Set pressure filter Y output value
         * @param value Y value to set
         */
        void setInY(float value) noexcept;

        /**
         * @brief Get pressure filter old value
         * @return Previous output value for derivative calculation
         */
        float inOld() const noexcept;

        /**
         * @brief Set pressure filter old value
         * @param value Old value to set
         */
        void setInOld(float value) noexcept;

        /**
         * @brief Get pressure filter sum value
         * @return Current sum for weighted averaging
         */
        float inSum() const noexcept;

        /**
         * @brief Set pressure filter sum value
         * @param value Sum value to set
         */
        void setInSum(float value) noexcept;

         /** @} */
         /** @} */

         /// @name System Version
         /// @{

         /**
          * @brief Get system version string
          * @return C-string containing version information
          */
         const char* sysVersion() const noexcept;

         /// @}

         /**
          * @brief Get filtered input pressure value
          * @return Current filtered pressure reading
          */
         float inputPressureFilter() const noexcept;

         /**
          * @brief Get weight before brew started
          * @return Pre-brew weight in grams
          */
         float preBrewWeight() const noexcept;

         /**
          * @brief Set weight before brew started
          * @param weight Pre-brew weight to set
          */
         void setPreBrewWeight(float weight) noexcept;

         /// @}

         /**
        /**



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
    // ===== COORDINATOR & CONTEXT MEMBERS =====
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

    // ===== PROCESS STATE MEMBERS =====
    double process_temperature_ = 0.0;
    double process_setpoint_ = 95.0;
    double process_pidOutput_ = 0.0;
    bool process_pidEnabled_ = true;
    double process_currBrewTime_ = 0.0;
    long process_startingTime_ = 0;
    double process_totalTargetBrewTime_ = 0.0;
    double process_steamSetpointValue_ = 120.0;
    bool process_brewPidDisabled_ = false;
    double process_previousInput_ = 0.0;
    double process_aggbKi_ = 0.0;
    double process_aggbKd_ = 0.0;
    double process_aggKi_ = 0.0;
    double process_aggKd_ = 0.0;
    int process_windowSize_ = 1000;

    // ===== COORDINATION STATE MEMBERS =====
    bool coordination_temperatureUpdateRunning_ = false;
    bool coordination_websiteUpdateRunning_ = false;
    bool coordination_hassioUpdateRunning_ = false;
    bool coordination_displayUpdateRunning_ = false;
    bool coordination_displayBufferReady_ = false;
    bool coordination_setupDone_ = false;

    // ===== NETWORK STATE MEMBERS =====
    bool network_offlineMode_ = false;
    unsigned int network_wifiReconnects_ = 0;
    unsigned long network_lastWifiConnectionAttempt_ = 0;
    unsigned long network_lastTempEvent_ = 0;
    unsigned long network_tempEventInterval_ = 1000;
    std::map<const char*, const char*, cmp_str> network_mqttVars_;
    std::map<const char*, std::function<double()>, cmp_str> network_mqttSensors_;
    bool network_mqtt_was_connected_ = false;
    unsigned int network_MQTTReCnctCount_ = 0;
    unsigned long network_lastMQTTConnectionAttempt_ = 0;
    bool network_hassioFailed_ = false;

    // ===== TIMING STATE MEMBERS =====
    unsigned long timing_previousMillistemp_ = 0;
    unsigned long timing_previousMillisMQTT_ = 0;
    unsigned long timing_previousMillisPressure_ = 0;
    std::unique_ptr<MillisecondTimer> timing_loopWaterTank_ = nullptr;
    std::unique_ptr<MillisecondTimer> timing_hassioDiscoveryTimer_ = nullptr;
    std::unique_ptr<MillisecondTimer> timing_printDisplayTimer_ = nullptr;
    MillisecondTimer* timing_loopWaterTank2_ = nullptr;
    MillisecondTimer* timing_hassioDiscoveryTimer2_ = nullptr;
    MillisecondTimer* timing_printDisplayTimer2_ = nullptr;
    unsigned int timing_isrCounter_ = 0;
    unsigned long timing_windowStartTime_ = 0;

    // ===== STANDBY STATE MEMBERS =====
    unsigned long standby_standbyModeRemainingTimeMillis_ = 0;
    unsigned long standby_standbyModeStartTimeMillis_ = 0;
    unsigned long standby_standbyModeRemainingTimeDisplayOffMillis_ = TIME_TO_DISPLAY_OFF_MILLIS;
    unsigned long standby_lastStandbyTimeMillis_ = 0;
    unsigned long standby_timeSinceStandbyMillis_ = 0;

    // ===== SENSOR STATE MEMBERS =====
    float sensors_inputPressure_ = 0.0;
    float sensors_inputPressureFilter_ = 0.0;
    double sensors_currBrewWeight_ = 0.0;
    double sensors_currReadingWeight_ = 0.0;
    bool sensors_scaleFailure_ = false;
    bool sensors_scaleTareOn_ = false;
    bool sensors_scaleCalibrationOn_ = false;
    int sensors_shottimerCounter_ = 10;
    float sensors_preBrewWeight_ = 0.0;
    bool sensors_autoTareInProgress_ = false;
    unsigned long sensors_autoTareStartTime_ = 0;
    unsigned long sensors_lastScaleConnectionCheck_ = 0;
    unsigned long sensors_scaleConnectionFailureTime_ = 0;
    bool sensors_scaleConnectionLost_ = false;
    float sensors_lastValidWeight_ = 0.0;
    bool sensors_brewByWeightFallbackActive_ = false;
    int sensors_scaleReadErrorCount_ = 0;
    int sensors_scaleMaxRetries_ = 5;
    unsigned long sensors_lastScaleErrorTime_ = 0;
    unsigned long sensors_scaleErrorCooldownMs_ = 1000;
    bool sensors_scaleInErrorRecovery_ = false;
    float sensors_inX_ = 0.0f;
    float sensors_inY_ = 0.0f;
    float sensors_inOld_ = 0.0f;
    float sensors_inSum_ = 0.0f;
    uint8_t sensors_currStateSteamSwitch_ = 0;
    bool sensors_currStatePowerSwitchPressed_ = false;
    bool sensors_lastPowerSwitchPressed_ = false;
    unsigned long sensors_systemInitializedTime_ = 0;
    unsigned long sensors_firstSwitchPressTime_ = 0;
    bool sensors_trackingPressTime_ = false;
    SwitchState sensors_currBrewSwitchState_ = SwitchState::IDLE;
    uint8_t sensors_brewSwitchReading_ = LOW;
    uint8_t sensors_currReadingBrewSwitch_ = LOW;
    bool sensors_brewSwitchWasOff_ = false;
    SwitchState sensors_currHotWaterSwitchState_ = SwitchState::IDLE;
    uint8_t sensors_hotWaterSwitchReading_ = LOW;
    uint8_t sensors_currReadingHotWaterSwitch_ = LOW;
    double sensors_currPumpOnTime_ = 0.0;
    unsigned long sensors_pumpStartingTime_ = 0;
    int sensors_waterTankCheckConsecutiveReads_ = 0;

    // ===== MACHINE STATE MEMBERS =====
    MachineStateId machine_machineState_ = MachineStateId::INIT;
    MachineStateId machine_lastmachinestate_ = MachineStateId::INIT;
    int machine_lastmachinestatepid_ = -1;
    bool machine_emergencyStop_ = false;
    bool machine_steamON_ = false;
    bool machine_steamFirstON_ = false;
    bool machine_backflushOn_ = false;
    int machine_currBackflushCycles_ = 1;
    bool machine_waterTankFull_ = true;
    bool machine_systemInitialized_ = false;
    MachineStateFlags machine_flags_ = MachineStateFlags();
    hw_timer_t* machine_timer_ = nullptr;

    // ===== DISPLAY STATE MEMBERS =====
    int display_displayOffline_ = 0;

    // ===== DEBUG STATE MEMBERS =====
    String debug_hotWaterStateDebug_ = "off";
    String debug_lastHotWaterStateDebug_ = "off";

    // ===== SYSTEM-WIDE REFERENCES =====
    Config* config_ = nullptr;
    PID* pid_ = nullptr;
    const char* sysVersion_ = VERSION;
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
