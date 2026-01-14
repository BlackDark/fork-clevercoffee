#pragma once

#include "clevercoffee/context/HardwareContext.h"
#include "clevercoffee/context/ProcessState.h"
#include "clevercoffee/context/SensorState.h"
#include "clevercoffee/context/TimingState.h"
#include "clevercoffee/coordinators/NetworkCoordinator.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/coordinators/StandbyCoordinator.h"
#include "clevercoffee/coordinators/UICoordinator.h"
#include "clevercoffee/types/GlobalTypes.h" // For type definitions (cmp_str, MachineStateFlags, etc.)
#include "clevercoffee/utils/ModernTimer.h" // For MillisecondTimer

#include <atomic>
#include <unordered_map>

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
 * // Note: Sensor updates are automatic and non-blocking via update()
 * double temp = ctx.sensorCoordinator().getTemperature();
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
    SensorCoordinator& sensorCoordinator() noexcept {
        return sensorCoordinator_;
    }

    /**
     * @brief Access sensor coordinator (const overload)
     *
     * @return Const reference to the sensor coordinator
     */
    const SensorCoordinator& sensorCoordinator() const noexcept {
        return sensorCoordinator_;
    }

    /**
     * @brief Access network coordinator
     *
     * Provides access to the NetworkCoordinator for managing network state.
     *
     * @return Reference to the network coordinator
     */
    NetworkCoordinator& networkCoordinator() noexcept {
        return networkCoordinator_;
    }

    /**
     * @brief Access network coordinator (const overload)
     *
     * @return Const reference to the network coordinator
     */
    const NetworkCoordinator& networkCoordinator() const noexcept {
        return networkCoordinator_;
    }

    /**
     * @brief Access UI coordinator
     *
     * Provides access to the UICoordinator for managing display operations.
     *
     * @return Reference to the UI coordinator
     */
    UICoordinator& uiCoordinator() noexcept {
        return uiCoordinator_;
    }

    /**
     * @brief Access UI coordinator (const overload)
     *
     * @return Const reference to the UI coordinator
     */
    const UICoordinator& uiCoordinator() const noexcept {
        return uiCoordinator_;
    }

    /**
     * @brief Access standby coordinator
     *
     * Provides access to the StandbyCoordinator for managing standby mode.
     *
     * @return Reference to the standby coordinator
     */
    StandbyCoordinator& standbyCoordinator() noexcept {
        return standbyCoordinator_;
    }

    /**
     * @brief Access standby coordinator (const overload)
     *
     * @return Const reference to the standby coordinator
     */
    const StandbyCoordinator& standbyCoordinator() const noexcept {
        return standbyCoordinator_;
    }

    /**
     * @brief Access hardware context
     *
     * Provides access to the HardwareContext for accessing hardware components.
     *
     * @return Reference to the hardware context
     */
    HardwareContext& hardwareContext() noexcept {
        return hardwareContext_;
    }

    /**
     * @brief Access hardware context (const overload)
     *
     * @return Const reference to the hardware context
     */
    const HardwareContext& hardwareContext() const noexcept {
        return hardwareContext_;
    }

    /** @} */

    /**
     * @name Handler Registration and Access
     * @{
     */

    /**
     * @brief Register brew handler
     * @param handler Pointer to BrewHandler instance (can be nullptr)
     */
    void setBrewHandler(BrewHandler* handler) noexcept {
        brewHandler_ = handler;
    }

    /**
     * @brief Get brew handler (REQUIRED - always exists after initialization)
     * @return Reference to BrewHandler
     */
    BrewHandler& brewHandler() noexcept {
        if (!brewHandler_) {
            LOG(FATAL, "BrewHandler not initialized - system bug!");
        }
        return *brewHandler_;
    }

    /**
     * @brief Get brew handler (const, REQUIRED - always exists after initialization)
     * @return Const reference to BrewHandler
     */
    const BrewHandler& brewHandler() const noexcept {
        if (!brewHandler_) {
            LOG(FATAL, "BrewHandler not initialized - system bug!");
        }
        return *brewHandler_;
    }

    /**
     * @brief Register hot water handler (REQUIRED - always set during initialization)
     * @param handler Pointer to HotWaterHandler instance (must not be nullptr)
     */
    void setHotWaterHandler(HotWaterHandler* handler) noexcept {
        hotWaterHandler_ = handler;
    }

    /**
     * @brief Get hot water handler (REQUIRED - always exists after initialization)
     * @return Reference to HotWaterHandler
     */
    HotWaterHandler& hotWaterHandler() noexcept {
        if (!hotWaterHandler_) {
            LOG(FATAL, "HotWaterHandler not initialized - system bug!");
        }
        return *hotWaterHandler_;
    }

    /**
     * @brief Get hot water handler (const, REQUIRED - always exists after initialization)
     * @return Const reference to HotWaterHandler
     */
    const HotWaterHandler& hotWaterHandler() const noexcept {
        if (!hotWaterHandler_) {
            LOG(FATAL, "HotWaterHandler not initialized - system bug!");
        }
        return *hotWaterHandler_;
    }

    /**
     * @brief Register power handler (REQUIRED - always set during initialization)
     * @param handler Pointer to PowerHandler instance (must not be nullptr)
     */
    void setPowerHandler(PowerHandler* handler) noexcept {
        powerHandler_ = handler;
    }

    /**
     * @brief Get power handler (REQUIRED - always exists after initialization)
     * @return Reference to PowerHandler
     */
    PowerHandler& powerHandler() noexcept {
        if (!powerHandler_) {
            LOG(FATAL, "PowerHandler not initialized - system bug!");
        }
        return *powerHandler_;
    }

    /**
     * @brief Get power handler (const, REQUIRED - always exists after initialization)
     * @return Const reference to PowerHandler
     */
    const PowerHandler& powerHandler() const noexcept {
        if (!powerHandler_) {
            LOG(FATAL, "PowerHandler not initialized - system bug!");
        }
        return *powerHandler_;
    }

    /**
     * @brief Register steam handler (REQUIRED - always set during initialization)
     * @param handler Pointer to SteamHandler instance (must not be nullptr)
     */
    void setSteamHandler(SteamHandler* handler) noexcept {
        steamHandler_ = handler;
    }

    /**
     * @brief Get steam handler (REQUIRED - always exists after initialization)
     * @return Reference to SteamHandler
     */
    SteamHandler& steamHandler() noexcept {
        if (!steamHandler_) {
            LOG(FATAL, "SteamHandler not initialized - system bug!");
        }
        return *steamHandler_;
    }

    /**
     * @brief Get steam handler (const, REQUIRED - always exists after initialization)
     * @return Const reference to SteamHandler
     */
    const SteamHandler& steamHandler() const noexcept {
        if (!steamHandler_) {
            LOG(FATAL, "SteamHandler not initialized - system bug!");
        }
        return *steamHandler_;
    }

    /**
     * @brief Register process controller
     * @param controller Pointer to ProcessController instance (can be nullptr)
     */
    void setProcessController(ProcessController* controller) noexcept {
        processController_ = controller;
    }

    /**
     * @brief Get process controller
     * @return Pointer to ProcessController (may be nullptr if not registered)
     */
    ProcessController* processController() noexcept {
        return processController_;
    }

    /**
     * @brief Get process controller (const)
     * @return Const pointer to ProcessController (may be nullptr if not registered)
     */
    const ProcessController* processController() const noexcept {
        return processController_;
    }

    /**
     * @brief Register machine state context
     * @param context Pointer to MachineStateContext instance (can be nullptr)
     */
    void setMachineStateContext(MachineStateContext* context) noexcept {
        machineStateContext_ = context;
    }

    /**
     * @brief Get machine state context
     * @return Pointer to MachineStateContext (may be nullptr if not registered)
     */
    MachineStateContext* machineStateContext() noexcept {
        return machineStateContext_;
    }

    /**
     * @brief Get machine state context (const)
     * @return Const pointer to MachineStateContext (may be nullptr if not registered)
     */
    const MachineStateContext* machineStateContext() const noexcept {
        return machineStateContext_;
    }

    /**
     * @brief Get process state (for direct access to ProcessState)
     * @return Reference to ProcessState
     */
    ProcessState& processState() noexcept {
        return processState_;
    }

    /**
     * @brief Get process state (const)
     * @return Const reference to ProcessState
     */
    const ProcessState& processState() const noexcept {
        return processState_;
    }

    /**
     * @brief Get sensor state (for direct access to SensorState)
     * @return Reference to SensorState
     */
    SensorState& sensorState() noexcept {
        return sensorState_;
    }

    /**
     * @brief Get sensor state (const)
     * @return Const reference to SensorState
     */
    const SensorState& sensorState() const noexcept {
        return sensorState_;
    }

    /**
     * @brief Get timing state (for direct access to TimingState)
     * @return Reference to TimingState
     */
    TimingState& timingState() noexcept {
        return timingState_;
    }

    /**
     * @brief Get timing state (const)
     * @return Const reference to TimingState
     */
    const TimingState& timingState() const noexcept {
        return timingState_;
    }

    /**
     * @name Network Managers
     * @{
     */

    /**
     * @brief Register MQTT manager
     * @param manager Pointer to MQTTManager instance (can be nullptr)
     */
    void setMQTTManager(MQTTManager* manager) noexcept {
        mqttManager_ = manager;
    }

    /**
     * @brief Get MQTT manager
     * @return Pointer to MQTTManager (may be nullptr if not registered)
     */
    MQTTManager* mqttManager() noexcept {
        return mqttManager_;
    }

    /**
     * @brief Get MQTT manager (const)
     * @return Const pointer to MQTTManager (may be nullptr if not registered)
     */
    const MQTTManager* mqttManager() const noexcept {
        return mqttManager_;
    }

    /**
     * @brief Register WiFi manager
     * @param manager Pointer to CleverCoffeeWiFiManager instance (can be nullptr)
     */
    void setCleverCoffeeWiFiManager(CleverCoffeeWiFiManager* manager) noexcept {
        cleverCoffeeWiFiManager_ = manager;
    }

    /**
     * @brief Get WiFi manager
     * @return Pointer to CleverCoffeeWiFiManager (may be nullptr if not registered)
     */
    CleverCoffeeWiFiManager* cleverCoffeeWiFiManager() noexcept {
        return cleverCoffeeWiFiManager_;
    }

    /**
     * @brief Get WiFi manager (const)
     * @return Const pointer to CleverCoffeeWiFiManager (may be nullptr if not registered)
     */
    const CleverCoffeeWiFiManager* cleverCoffeeWiFiManager() const noexcept {
        return cleverCoffeeWiFiManager_;
    }

    /**
     * @brief Register WebServer manager
     * @param manager Pointer to WebServerManager instance (can be nullptr)
     */
    void setWebServerManager(WebServerManager* manager) noexcept {
        webServerManager_ = manager;
    }

    /**
     * @brief Get WebServer manager
     * @return Pointer to WebServerManager (may be nullptr if not registered)
     */
    WebServerManager* webServerManager() noexcept {
        return webServerManager_;
    }

    /**
     * @brief Get WebServer manager (const)
     * @return Const pointer to WebServerManager (may be nullptr if not registered)
     */
    const WebServerManager* webServerManager() const noexcept {
        return webServerManager_;
    }

    /** @} */

    /** @} */

    /**
     * @name Process State Accessors
     * Centralized access to process state through SystemContext.
     * Implementations delegate to ProcessState for unified state management.
     *
     * @note See STATE_ACCESS_PATTERNS.md for detailed access patterns.
     * Process state includes PID values, temperature (with brew offset), setpoint, and brew timing.
     *
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
        double currentTemperature  = 0.0;   ///< Current measured temperature (°C)
        double setpointTemperature = 0.0;   ///< Target setpoint (°C)
        double pidOutputPercent    = 0.0;   ///< Heater power (0-1000)
        double currentBrewTime     = 0.0;   ///< Elapsed brew time (ms)
        double targetBrewTime      = 0.0;   ///< Target brew duration (ms)
        bool   brewPidDisabled     = false; ///< Brew PID active state

        // PID tuning values
        double pidKp = 0.0; ///< Proportional gain
        double pidKi = 0.0; ///< Integral gain
        double pidKd = 0.0; ///< Derivative gain

        // Sensor data
        double pumpOnTime    = 0.0;  ///< Hot water pump runtime (ms)
        float  inputPressure = 0.0f; ///< Current pump pressure (bar)
        double brewWeight    = 0.0;  ///< Current shot weight (grams)

        // Timing/animation
        unsigned int isrCounter = 0; ///< ISR counter for animation sync

        // Coordination flags
        bool displayBufferReady = false; ///< Display update ready flag
    };

    /**
     * @brief Get atomic snapshot of all display data
     * @return DisplaySnapshot containing current display state
     */
    DisplaySnapshot getDisplaySnapshot() const noexcept;

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
     * @brief Check if ISR is ready to execute
     * @return true if ISR can safely execute, false otherwise
     */
    bool isISRReady() const noexcept;

    /**
     * @brief Mark ISR as ready to execute
     * Should be called after all initialization is complete and system is ready
     */
    void markISRReady() noexcept;

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
     * @note These methods delegate to SensorCoordinator for consistency.
     * @deprecated Prefer using sensorCoordinator() methods directly:
     *   - Use sensorCoordinator().getBrewWeight() instead of currBrewWeight()
     *   - Use sensorCoordinator().getWeight() instead of currReadingWeight()
     *   - Use sensorCoordinator().isScaleCalibrationMode() instead of scaleCalibrationOn()
     *   - Use sensorCoordinator().isScaleTareMode() instead of scaleTareOn()
     * @{
     */

    /**
     * @brief Check if scale calibration is currently active
     * @return true if scale calibration mode is on
     * @deprecated Use sensorCoordinator().isScaleCalibrationMode() instead
     */
    bool scaleCalibrationOn() const noexcept;

    /**
     * @brief Set scale calibration active state
     * @param on true to activate calibration, false to deactivate
     * @deprecated Use sensorCoordinator().setScaleCalibrationMode() instead
     */
    void setScaleCalibrationOn(bool on) noexcept;

    /**
     * @brief Check if scale tare operation is active
     * @return true if scale tare is in progress
     * @deprecated Use sensorCoordinator().isScaleTareMode() instead
     */
    bool scaleTareOn() const noexcept;

    /**
     * @brief Set scale tare active state
     * @param on true to activate tare, false to deactivate
     * @deprecated Use sensorCoordinator().setScaleTareMode() instead
     */
    void setScaleTareOn(bool on) noexcept;

    /**
     * @brief Get current brew weight reading
     * @return Current weight in grams
     * @deprecated Use sensorCoordinator().getBrewWeight() instead
     */
    double currBrewWeight() const noexcept;

    /**
     * @brief Set current brew weight
     * @param weight Weight in grams
     * @note This is managed by SensorCoordinator - setting directly may be overwritten
     */
    void setCurrBrewWeight(double weight) noexcept;

    /**
     * @brief Get current scale reading weight
     * @return Current weight reading in grams
     * @deprecated Use sensorCoordinator().getWeight() instead
     */
    double currReadingWeight() const noexcept;

    /**
     * @brief Set current scale reading weight
     * @param weight Weight in grams
     * @note This is managed by SensorCoordinator - setting directly may be overwritten
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
     * @deprecated Use sensorCoordinator().getPressure() instead
     */
    float inputPressure() const noexcept;

    /**
     * @brief Set input pressure value
     * @param pressure Pressure value
     * @note This is managed by SensorCoordinator - setting directly may be overwritten
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
     *
     * @note Pressure filtering is now handled internally by SensorCoordinator.
     * These methods are kept for backward compatibility but may be removed in future.
     * Use sensorCoordinator().getFilteredPressure() for filtered pressure readings.
     * @{
     */

    /**
     * @brief Get pressure filter X input value
     * @return Current X (weighted input) value
     * @deprecated Pressure filter is internal to SensorCoordinator
     */
    float inX() const noexcept;

    /**
     * @brief Set pressure filter X input value
     * @param value X value to set
     * @deprecated Pressure filter is internal to SensorCoordinator
     */
    void setInX(float value) noexcept;

    /**
     * @brief Get pressure filter Y output value
     * @return Current Y (filtered output) value
     * @deprecated Pressure filter is internal to SensorCoordinator
     */
    float inY() const noexcept;

    /**
     * @brief Set pressure filter Y output value
     * @param value Y value to set
     * @deprecated Pressure filter is internal to SensorCoordinator
     */
    void setInY(float value) noexcept;

    /**
     * @brief Get pressure filter old value
     * @return Previous output value for derivative calculation
     * @deprecated Pressure filter is internal to SensorCoordinator
     */
    float inOld() const noexcept;

    /**
     * @brief Set pressure filter old value
     * @param value Old value to set
     * @deprecated Pressure filter is internal to SensorCoordinator
     */
    void setInOld(float value) noexcept;

    /**
     * @brief Get pressure filter sum value
     * @return Current sum for weighted averaging
     * @deprecated Pressure filter is internal to SensorCoordinator
     */
    float inSum() const noexcept;

    /**
     * @brief Set pressure filter sum value
     * @param value Sum value to set
     * @deprecated Pressure filter is internal to SensorCoordinator
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
     * @deprecated Use sensorCoordinator().getFilteredPressure() instead
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
    void markReady() noexcept {
        ready_ = true;
    }

    /**
     * @brief Check if system is fully initialized
     *
     * @return true if the system has completed initialization, false otherwise
     */
    bool isReady() const noexcept {
        return ready_;
    }

    /** @} */

  private:
    // ===== COORDINATOR & CONTEXT MEMBERS =====
    HardwareContext    hardwareContext_;    ///< Hardware component registry
    SensorCoordinator  sensorCoordinator_;  ///< Manages sensor operation state
    NetworkCoordinator networkCoordinator_; ///< Manages network connection state
    UICoordinator      uiCoordinator_;      ///< Manages UI refresh and sleep state
    StandbyCoordinator standbyCoordinator_; ///< Manages standby mode and power management
    bool               ready_ = false;      ///< System initialization complete flag

    // Handler references - ALL REQUIRED (always exist after initialization)
    BrewHandler*     brewHandler_     = nullptr; // Set during initializeHandlers()
    HotWaterHandler* hotWaterHandler_ = nullptr; // Set during initializeHandlers()
    PowerHandler*    powerHandler_    = nullptr; // Set during initializeHandlers()
    SteamHandler*    steamHandler_    = nullptr; // Set during initializeHandlers()

    // Controller reference (non-owning pointer)
    ProcessController* processController_ = nullptr;

    // Machine state context reference (non-owning pointer)
    MachineStateContext* machineStateContext_ = nullptr;

    // Manager references (non-owning pointers)
    MQTTManager*             mqttManager_             = nullptr;
    CleverCoffeeWiFiManager* cleverCoffeeWiFiManager_ = nullptr;
    WebServerManager*        webServerManager_        = nullptr;

    // ===== PROCESS STATE =====
    // NOTE: Process state is now managed by ProcessState class
    // All deprecated process_* members have been removed - use processState() methods
    ProcessState processState_;

    // ===== SENSOR STATE =====
    // NOTE: Sensor state is now managed by SensorState class
    SensorState sensorState_;

    // NOTE: Process state deprecated members removed - use processState() methods instead
    // Removed: process_temperature_, process_setpoint_, process_pidOutput_, process_pidEnabled_,
    //          process_currBrewTime_, process_startingTime_, process_totalTargetBrewTime_,
    //          process_steamSetpointValue_, process_brewPidDisabled_, process_previousInput_,
    //          process_aggbKi_, process_aggbKd_, process_aggKi_, process_aggKd_, process_windowSize_
    // These were verified unused and have been removed. Use processState() methods instead.

    // ===== COORDINATION STATE MEMBERS =====
    // NOTE: Coordination state is now managed by coordinators
    // NOTE: Coordination deprecated members removed - use coordinator methods instead
    // Removed: coordination_displayBufferReady_, coordination_setupDone_,
    //          coordination_temperatureUpdateRunning_, coordination_websiteUpdateRunning_,
    //          coordination_hassioUpdateRunning_, coordination_displayUpdateRunning_
    // These were verified unused and have been removed. Use coordinator methods instead.

    // ===== NETWORK STATE MEMBERS =====
    // NOTE: Network state is now managed by NetworkCoordinator
    // NOTE: Network state deprecated members removed - use networkCoordinator() methods instead
    // Removed: network_offlineMode_, network_wifiReconnects_, network_hassioFailed_,
    //          network_lastWifiConnectionAttempt_, network_lastMQTTConnectionAttempt_,
    //          network_MQTTReCnctCount_, network_mqttVars_, network_mqttSensors_,
    //          network_mqtt_was_connected_, network_lastTempEvent_, network_tempEventInterval_
    // These were verified unused and have been removed. Use networkCoordinator() methods instead.

    // ===== TIMING STATE =====
    // NOTE: Simple timing state is now managed by TimingState class
    // ISR-related timing and hardware timers remain in SystemContext for safety
    TimingState timingState_;

    // NOTE: Timing state members removed - use timingState() methods instead
    // Removed: timing_previousMillistemp_, timing_previousMillisMQTT_,
    //          timing_previousMillisPressure_, timing_windowStartTime_
    // These were verified unused and have been removed.

    // NOTE: Hardware timers remain in SystemContext due to initialization complexity
    // These are managed via unique_ptr and require careful lifecycle management
    std::unique_ptr<MillisecondTimer> timing_loopWaterTank_        = nullptr;
    std::unique_ptr<MillisecondTimer> timing_hassioDiscoveryTimer_ = nullptr;
    std::unique_ptr<MillisecondTimer> timing_printDisplayTimer_    = nullptr;

    // NOTE: Removed unused legacy timer pointers: timing_loopWaterTank2_,
    //       timing_hassioDiscoveryTimer2_, timing_printDisplayTimer2_
    // These were verified unused and have been removed.

    // NOTE: ISR-related timing remains in SystemContext due to ISR access requirements
    // ISR code needs direct access to these members for performance and safety
    unsigned int      timing_isrCounter_ = 0; // Accessed from ISR - must remain in SystemContext
    std::atomic<bool> timing_isrReady_{
        false}; ///< ISR ready flag - prevents ISR from executing before system is initialized

    // ===== STANDBY STATE MEMBERS =====
    // NOTE: Standby state is now managed by StandbyCoordinator
    // NOTE: Standby state deprecated members removed - use standbyCoordinator() methods instead
    // Removed: standby_standbyModeRemainingTimeMillis_, standby_standbyModeStartTimeMillis_,
    //          standby_standbyModeRemainingTimeDisplayOffMillis_, standby_lastStandbyTimeMillis_,
    //          standby_timeSinceStandbyMillis_
    // These were verified unused and have been removed. Use standbyCoordinator() methods instead.

    // ===== SENSOR STATE MEMBERS =====
    // NOTE: Sensor state is now managed by SensorState class
    // NOTE: Sensor state deprecated members removed - use sensorState() methods instead
    // Removed: All sensors_* members (30+ members including pressure, weight, scale, switches, etc.)
    // These were verified unused and have been removed. Use sensorState() methods instead.

    // ===== MACHINE STATE MEMBERS =====
    // NOTE: Machine state is now managed by MachineStateContext
    // NOTE: Machine state deprecated members removed - use machineStateContext() methods instead
    // Removed: machine_emergencyStop_, machine_steamON_, machine_steamFirstON_, machine_backflushOn_,
    //          machine_currBackflushCycles_, machine_waterTankFull_, machine_systemInitialized_,
    //          machine_machineState_, machine_lastmachinestate_, machine_lastmachinestatepid_, machine_flags_
    // These were verified unused and have been removed. Use machineStateContext() methods instead.
    hw_timer_t* machine_timer_ = nullptr; // NOTE: ISR-accessed hardware timer, must remain in SystemContext

    // ===== DISPLAY STATE MEMBERS =====
    // NOTE: Removed unused member: display_displayOffline_
    // This was verified unused and has been removed. Use uiCoordinator_.getDisplayOffline() instead.

    // ===== DEBUG STATE MEMBERS =====
    // NOTE: Removed unused debug strings: debug_hotWaterStateDebug_, debug_lastHotWaterStateDebug_
    // These were verified unused and have been removed.

    // ===== SYSTEM-WIDE REFERENCES =====
    Config*     config_     = nullptr;
    PID*        pid_        = nullptr;
    const char* sysVersion_ = VERSION;
};

// Global SystemContext accessor removed - use dependency injection instead.
// For ISR code, use CleverCoffee::ISR::getSystemContext() from isr.h

} // namespace CleverCoffee
