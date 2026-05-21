/**
 * @file MachineStateContext.h
 * @brief Context class providing access to all machine resources for state implementations
 */

#pragma once

#include "clevercoffee/state/IConfigContext.h"
#include "clevercoffee/state/IHardwareContext.h"
#include "clevercoffee/state/IStateManager.h"
#include "clevercoffee/state/MachineStateIds.h"

#include <chrono>
#include <memory>

// Forward declarations
class Config;
class DisplayManager;
class MQTTManager;
class IWiFiManager;
class U8G2;
class TempSensor;
class Switch;
class Relay;
class LED;
class Scale;

namespace CleverCoffee {
class HardwareManager;
class SystemContext;
class SensorCoordinator;
class MachineState;
} // namespace CleverCoffee

/**
 * @class MachineStateContext
 * @brief Provides unified access to all machine resources and state data
 *
 * The MachineStateContext serves as the interface between state implementations
 * and the coffee machine's hardware, sensors, configuration, and control systems.
 * It encapsulates all necessary data and provides clean APIs for state logic.
 *
 * Implements three interfaces to break circular dependencies:
 * - IHardwareContext: Hardware component access
 * - IConfigContext: Configuration parameter access
 * - IStateManager: State transition and timing management
 */
class MachineStateContext : public CleverCoffee::IHardwareContext,
                            public CleverCoffee::IConfigContext,
                            public CleverCoffee::IStateManager {
  public:
    /**
     * @brief Construct context with references to all managers
     * @param systemContext System context (REQUIRED)
     * @param hardwareManager Hardware manager (REQUIRED - CRITICAL component)
     * @param displayManager Display manager (REQUIRED - always exists)
     * @param wifiManager WiFi manager (REQUIRED - always exists)
     * @param mqttManager MQTT manager (REQUIRED - always exists)
     */
    MachineStateContext(CleverCoffee::SystemContext&   systemContext,
                        CleverCoffee::HardwareManager& hardwareManager,
                        DisplayManager&                displayManager,
                        IWiFiManager&                  wifiManager,
                        MQTTManager&                   mqttManager);

    /**
     * @brief Virtual destructor for proper cleanup
     */
    virtual ~MachineStateContext() = default;

    // === Hardware Access ===

    /**
     * @brief Get system context
     */
    CleverCoffee::SystemContext& systemContext() noexcept {
        return systemContext_;
    }
    const CleverCoffee::SystemContext& systemContext() const noexcept {
        return systemContext_;
    }

    /**
     * @brief Get display manager
     * @return Pointer to display manager (always exists when enabled, nullptr when disabled)
     */
    DisplayManager& getDisplayManager() const noexcept {
        return displayManager_;
    }

    /**
     * @brief Get hardware manager (REQUIRED - CRITICAL component)
     */
    CleverCoffee::HardwareManager& getHardwareManager() const noexcept {
        return hardwareManager_;
    }

    /**
     * @brief Get WiFi manager
     */
    /**
     * @brief Get WiFi manager
     * @return Pointer to WiFi manager (always exists when enabled, nullptr when offline mode)
     */
    IWiFiManager& getWiFiManager() const noexcept {
        return wifiManager_;
    }

    /**
     * @brief Get MQTT manager
     * @return Pointer to MQTT manager (always exists when enabled, nullptr when disabled)
     */
    MQTTManager& getMQTTManager() const noexcept {
        return mqttManager_;
    }

    // === Hardware Component Access ===

    /**
     * @brief Get temperature sensor
     */
    TempSensor*       getTempSensor() noexcept override;
    const TempSensor* getTempSensor() const noexcept override;

    /**
     * @brief Get water tank sensor
     */
    Switch*       getWaterTankSensor() noexcept override;
    const Switch* getWaterTankSensor() const noexcept override;

    /**
     * @brief Get brew switch
     */
    Switch* getBrewSwitch() const;

    /**
     * @brief Get steam switch
     */
    Switch* getSteamSwitch() const;

    /**
     * @brief Get hot water switch
     */
    Switch* getHotWaterSwitch() const;

    /**
     * @brief Get power switch
     */
    Switch* getPowerSwitch() const;

    /**
     * @brief Get heater relay
     */
    Relay*       getHeaterRelay() noexcept override;
    const Relay* getHeaterRelay() const;

    /**
     * @brief Get pump relay
     */
    Relay*       getPumpRelay() noexcept override;
    const Relay* getPumpRelay() const;

    /**
     * @brief Get valve relay
     */
    Relay*       getValveRelay() noexcept override;
    const Relay* getValveRelay() const;

    /**
     * @brief Get status LED
     */
    LED*       getStatusLed() noexcept override;
    const LED* getStatusLed() const noexcept override;

    /**
     * @brief Get brew LED
     */
    LED* getBrewLED() const;

    /**
     * @brief Get steam LED
     */
    LED* getSteamLED() const;

    /**
     * @brief Get scale
     */
    Scale* getScale() const;

    // === Sensor Data Access ===

    /**
     * @brief Get current temperature reading
     */
    double getCurrentTemperature() const noexcept;

    /**
     * @brief Check if temperature sensor has error
     */
    bool hasTemperatureError() const noexcept;

    /**
     * @brief Check if water tank is full
     */
    bool isWaterTankFull() const;

    /**
     * @brief Get current pressure reading
     */
    float getCurrentPressure() const;

    /**
     * @brief Get filtered pressure reading
     */
    float getFilteredPressure() const;

    /**
     * @brief Get current weight reading
     */
    float getCurrentWeight() const noexcept;

    /**
     * @brief Get current brew weight
     */
    float getCurrentBrewWeight() const noexcept;

    /**
     * @brief Check if scale has error
     */
    bool hasScaleError() const;

    /**
     * @brief Check if any sensors have errors
     */
    bool hasSensorError() const;

    // === Process Control Functions ===

    /**
     * @brief Check if brew process is active
     */
    bool isBrewActive() const;

    /**
     * @brief Check if manual flush is active
     */
    bool isManualFlushActive() const;

    /**
     * @brief Check if steam is active
     */
    bool isSteamActive() const;

    /**
     * @brief Check if hot water process is active
     */
    bool isHotWaterActive() const;

    /**
     * @brief Check if backflush is active
     */
    bool isBackflushActive() const;

    // === System State Access ===

    /**
     * @brief Check if PID is enabled
     */
    bool isPidEnabled() const;

    /**
     * @brief Check if emergency stop is active
     */
    bool isEmergencyStop() const;

    /**
     * @brief Check if standby mode should activate
     */
    bool shouldEnterStandby() const;

    /**
     * @brief Get standby remaining time
     */
    unsigned long getStandbyRemainingTime() const;

    // === Timing Functions ===

    /**
     * @brief Get current time in milliseconds
     */
    unsigned long getCurrentTime() const;

    /**
     * @brief Reset standby timer for given state
     */
    void resetStandbyTimer(MachineStateId stateId) const;

    /**
     * @brief Initialize standby timer if not already initialized
     * Should be called when standby is enabled to start the countdown
     */
    void initializeStandbyTimerIfNeeded() const;

    // === Control Functions ===

    /**
     * @brief Set steam mode
     */
    void setSteamMode(bool enabled) const;

    /**
     * @brief Set PID runtime state
     */
    void setPidRuntimeState(bool enabled) const;

    /**
     * @brief Set manual flush state
     */
    void setManualFlushState(bool active) const;

    /**
     * @brief Set steam state
     */
    void setSteamState(bool active);

    /**
     * @brief Set backflush state
     */
    void setBackflushState(bool active);

    /**
     * @brief Disable water-dependent operations for safety
     */
    void disableWaterOperations() const;

    /**
     * @brief Enable water-dependent operations
     */
    void enableWaterOperations() const;

    /**
     * @brief Enter safe mode (disable critical operations)
     */
    void enterSafeMode() const;

    /**
     * @brief Exit safe mode (re-enable operations)
     */
    void exitSafeMode() const;

    /**
     * @brief Enter standby mode (power saving)
     */
    void enterStandbyMode() const;

    /**
     * @brief Exit standby mode
     */
    void exitStandbyMode() const;

    /**
     * @brief Check if user activity detected
     */
    bool hasUserActivity() const;

    /**
     * @brief Check if should exit standby
     */
    bool shouldExitStandby() const;

    /**
     * @brief Perform safe shutdown
     */
    void performSafeShutdown() const;

    // === Machine State Flags Access ===

    /**
     * @brief Check if emergency stop is active
     */
    bool isEmergencyStopActive() const noexcept {
        return emergencyStop_;
    }

    /**
     * @brief Set emergency stop state
     */
    void setEmergencyStop(bool active) noexcept {
        emergencyStop_ = active;
    }

    /**
     * @brief Get steam mode state
     */
    bool isSteamModeActive() const noexcept {
        return steamON_;
    }

    /**
     * @brief Set steam mode state
     */
    void setSteamModeActive(bool active) noexcept {
        steamON_ = active;
    }

    /**
     * @brief Check if steam was activated for first time
     */
    bool isSteamFirstActivated() const noexcept {
        return steamFirstON_;
    }

    /**
     * @brief Set steam first activation flag
     */
    void setSteamFirstActivated(bool active) noexcept {
        steamFirstON_ = active;
    }

    /**
     * @brief Check if backflush is active
     */
    bool isBackflushModeActive() const noexcept {
        return backflushOn_;
    }

    /**
     * @brief Enable or disable backflush mode and request matching state transitions
     */
    void applyBackflushMode(bool active) noexcept;

    /**
     * @brief Get current backflush cycle count
     */
    int getBackflushCycleCount() const noexcept {
        return currBackflushCycles_;
    }

    /**
     * @brief Set backflush cycle count
     */
    void setBackflushCycleCount(int cycles) noexcept {
        currBackflushCycles_ = cycles;
    }

    /**
     * @brief Check if water tank is full
     */
    bool isWaterTankFullState() const noexcept {
        return waterTankFull_;
    }

    /**
     * @brief Set water tank full state
     */
    void setWaterTankFullState(bool full) noexcept {
        waterTankFull_ = full;
    }

    /**
     * @brief Check if system is initialized
     */
    bool isSystemInitialized() const noexcept {
        return systemInitialized_;
    }

    /**
     * @brief Set system initialization state
     */
    void setSystemInitialized(bool initialized) noexcept {
        systemInitialized_ = initialized;
    }

    // === Request Flags for State Transitions ===

    /**
     * @brief Check if brew start is requested
     */
    bool isBrewStartRequested() const noexcept {
        return requestBrewStart_;
    }

    /**
     * @brief Set brew start request
     */
    void setBrewStartRequested(bool requested) noexcept;

    /**
     * @brief Check if brew stop is requested
     */
    bool isBrewStopRequested() const noexcept {
        return requestBrewStop_;
    }

    /**
     * @brief Set brew stop request
     */
    void setBrewStopRequested(bool requested) noexcept {
        requestBrewStop_ = requested;
    }

    /**
     * @brief Check if steam start is requested
     */
    bool isSteamStartRequested() const noexcept {
        return requestSteamStart_;
    }

    /**
     * @brief Set steam start request
     */
    void setSteamStartRequested(bool requested) noexcept;

    /**
     * @brief Check if steam stop is requested
     */
    bool isSteamStopRequested() const noexcept {
        return requestSteamStop_;
    }

    /**
     * @brief Set steam stop request
     */
    void setSteamStopRequested(bool requested) noexcept {
        requestSteamStop_ = requested;
    }

    /**
     * @brief Check if manual flush start is requested
     */
    bool isManualFlushStartRequested() const noexcept {
        return requestManualFlushStart_;
    }

    /**
     * @brief Set manual flush start request
     */
    void setManualFlushStartRequested(bool requested) noexcept {
        requestManualFlushStart_ = requested;
    }

    /**
     * @brief Check if manual flush stop is requested
     */
    bool isManualFlushStopRequested() const noexcept {
        return requestManualFlushStop_;
    }

    /**
     * @brief Set manual flush stop request
     */
    void setManualFlushStopRequested(bool requested) noexcept {
        requestManualFlushStop_ = requested;
    }

    /**
     * @brief Check if entering backflush mode (PID → BACKFLUSH_IDLE) is requested
     */
    bool isBackflushEnterRequested() const noexcept {
        return requestEnterBackflush_;
    }

    /**
     * @brief Set enter-backflush request (UI/MQTT toggle on)
     */
    void setBackflushEnterRequested(bool requested) noexcept;

    /**
     * @brief Check if starting a backflush fill/flush cycle is requested
     */
    bool isBackflushCycleStartRequested() const noexcept {
        return requestBackflushCycleStart_;
    }

    /**
     * @brief Set backflush cycle start request (brew switch in idle/finished)
     */
    void setBackflushCycleStartRequested(bool requested) noexcept;

    /**
     * @brief Check if backflush stop is requested
     */
    bool isBackflushStopRequested() const noexcept {
        return requestBackflushStop_;
    }

    /**
     * @brief Set backflush stop request
     */
    void setBackflushStopRequested(bool requested) noexcept;

    /**
     * @brief Check if standby is requested
     */
    bool isStandbyRequested() const noexcept {
        return requestStandby_;
    }

    /**
     * @brief Set standby request
     */
    void setStandbyRequested(bool requested) noexcept {
        requestStandby_ = requested;
    }

    /**
     * @brief Check if normal operation is requested
     */
    bool isNormalOperationRequested() const noexcept {
        return requestNormalOperation_;
    }

    /**
     * @brief Set normal operation request
     */
    void setNormalOperationRequested(bool requested) noexcept;

    /**
     * @brief Set hot water activity (user interaction detected)
     * Resets standby timer when active
     */
    void setHotWaterActivity(bool active) noexcept;

    // === Display Functions ===

    /**
     * @brief Get U8G2 display instance
     */
    U8G2* getDisplay() const;

    /**
     * @brief Set display power save mode
     */
    void setDisplayPowerSave(int mode) const;

    // === Logging Functions ===

    /**
     * @brief Log state transition
     */
    void logStateTransition(MachineStateId fromState, MachineStateId toState, const char* reason = nullptr) const;

    /**
     * @brief Get the appropriate PID state based on PID enabled status
     * @return MachineStateId::PID_NORMAL if PID is enabled, PID_DISABLED otherwise
     *
     * This is a helper function to reduce code duplication in state transitions.
     * Many states need to transition back to PID state, and this centralizes the logic.
     */
    MachineStateId getPidState() const noexcept;

    /**
     * @brief Log state entry
     */
    void logStateEntry(MachineStateId stateId, const char* stateName) const;

    /**
     * @brief Log state exit
     */
    void logStateExit(MachineStateId stateId, const char* stateName) const;

    // === MQTT Integration ===

    /**
     * @brief Check if MQTT reconnection count should be reset
     */
    void resetMqttReconnectCount() const;

    // === Configuration Access ===

    /**
     * @brief Get backflush fill time from config (in milliseconds)
     */
    unsigned long getBackflushFillTimeMs() const;

    /**
     * @brief Get backflush flush time from config (in milliseconds)
     */
    unsigned long getBackflushFlushTimeMs() const;

    // === State Timing Functions ===

    /**
     * @brief Get time elapsed since current state was entered (in milliseconds)
     */
    unsigned long getStateElapsedTimeMs() const;

    /**
     * @brief Check if the specified timeout has elapsed since current state entry
     * @param timeoutMs Timeout in milliseconds
     * @return true if timeout has elapsed
     */
    bool hasStateTimeoutElapsed(unsigned long timeoutMs) const noexcept;

    /**
     * @brief Update the state entry time (called by StateMachine on state transitions)
     * @param entryTime Time when the state was entered
     */
    void updateStateEntryTime(std::chrono::steady_clock::time_point entryTime);

    // === IHardwareContext Interface Implementation ===

    // Hardware sensors
    bool   isWaterTankEmpty() const noexcept override;
    double getWeight() const noexcept override;
    void   tareScale() noexcept override;
    void   updateHardware() noexcept override;

    // High-level hardware control (delegated to HardwareManager)
    void enableHeater() noexcept override;
    void disableHeater() noexcept override;
    void setHeaterPower(uint8_t percentage) noexcept override;
    void enablePump() noexcept override;
    void disablePump() noexcept override;
    void setPumpPressure(float bar) noexcept override;
    void openSteamValve() noexcept override;
    void closeSteamValve() noexcept override;
    void openWaterValve() noexcept override;
    void closeWaterValve() noexcept override;
    void openSolenoid() noexcept override;
    void closeSolenoid() noexcept override;
    void emergencyShutdown() noexcept override;

    // === IConfigContext Interface Implementation ===

    double        getBrewSetpoint() const noexcept override;
    double        getSteamSetpoint() const noexcept override;
    double        getTargetBrewTime() const noexcept override;
    double        getPreInfusionTime() const noexcept override;
    double        getPidKp() const noexcept override;
    double        getPidTn() const noexcept override;
    double        getPidTv() const noexcept override;
    Config&       getConfig() noexcept override;
    const Config& getConfig() const noexcept override;

    // === IStateManager Interface Implementation ===

    MachineStateId getCurrentStateId() const noexcept override;
    void           setCurrentStateId(MachineStateId stateId) noexcept {
        currentStateId_ = stateId;
    }
    void          transitionTo(MachineStateId newStateId) override;
    unsigned long getStateStartTime() const noexcept override;

  private:
    /**
     * @brief Reset standby timer on user activity
     * Called automatically when user activity flags are set
     */
    void resetStandbyTimerOnUserActivity() const;
    // System context
    CleverCoffee::SystemContext& systemContext_;

    // Manager references - ALL REQUIRED
    CleverCoffee::HardwareManager& hardwareManager_; // REQUIRED - CRITICAL component
    DisplayManager&                displayManager_;  // REQUIRED - always exists
    IWiFiManager&                  wifiManager_;     // REQUIRED - always exists
    MQTTManager&                   mqttManager_;     // REQUIRED - always exists

                                                     // === Machine State Management ===
    bool emergencyStop_       = false; ///< Emergency stop activated
    bool steamON_             = false; ///< Steam mode active
    bool steamFirstON_        = false; ///< Steam activated for first time
    bool backflushOn_         = false; ///< Backflush mode active
    int  currBackflushCycles_ = 1;     ///< Current backflush cycle count
    bool waterTankFull_       = true;  ///< Water tank full state
    bool systemInitialized_   = false; ///< System initialization complete

    // === State Transition Request Flags ===
    bool requestBrewStart_           = false; ///< Brew start requested
    bool requestBrewStop_            = false; ///< Brew stop requested
    bool requestSteamStart_          = false; ///< Steam start requested
    bool requestSteamStop_           = false; ///< Steam stop requested
    bool requestManualFlushStart_    = false; ///< Manual flush start requested
    bool requestManualFlushStop_     = false; ///< Manual flush stop requested
    bool requestEnterBackflush_      = false; ///< Enter backflush mode (→ BACKFLUSH_IDLE)
    bool requestBackflushCycleStart_ = false; ///< Start fill/flush cycle
    bool requestBackflushStop_       = false; ///< Backflush stop requested
    bool requestStandby_             = false; ///< Standby requested
    bool requestNormalOperation_     = false; ///< Normal operation requested

    // State management
    MachineStateId currentStateId_ = MachineStateId::PID_DISABLED; ///< Current machine state ID

    // State timing
    std::chrono::steady_clock::time_point stateEntryTime_; ///< Time when current state was entered
};
