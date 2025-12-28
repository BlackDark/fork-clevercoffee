/**
 * @file MachineStateContext.h
 * @brief Context class providing access to all machine resources for state implementations
 */

#pragma once

#include "clevercoffee/state/IHardwareContext.h"
#include "clevercoffee/state/IConfigContext.h"
#include "clevercoffee/state/IStateManager.h"
#include "clevercoffee/state/MachineStateIds.h"

#include <chrono>
#include <memory>

// Forward declarations
class Config;
class DisplayManager;
class SensorManager;
class MQTTManager;
class CleverCoffeeWiFiManager;
class U8G2;
class TempSensor;
class Switch;
class Relay;
class LED;
class Scale;

namespace CleverCoffee {
class HardwareManager;
class SystemContext;
class MachineState;
}

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
     */
    MachineStateContext(CleverCoffee::SystemContext& systemContext,
                        DisplayManager*               displayManager,
                        CleverCoffee::HardwareManager*              hardwareManager,
                        SensorManager*                sensorManager,
                        CleverCoffeeWiFiManager*      wifiManager,
                        MQTTManager*                  mqttManager);

    /**
     * @brief Virtual destructor for proper cleanup
     */
    virtual ~MachineStateContext() = default;

    // === Hardware Access ===

    /**
     * @brief Get system context
     */
    CleverCoffee::SystemContext& systemContext() noexcept { return systemContext_; }
    const CleverCoffee::SystemContext& systemContext() const noexcept { return systemContext_; }

    /**
     * @brief Get display manager
     */
    DisplayManager* getDisplayManager() const noexcept {
        return displayManager_;
    }

    /**
     * @brief Get hardware manager
     */
    CleverCoffee::HardwareManager* getHardwareManager() const noexcept {
        return hardwareManager_;
    }

    /**
     * @brief Get sensor manager
     */
    SensorManager* getSensorManager() const noexcept {
        return sensorManager_;
    }

    /**
     * @brief Get WiFi manager
     */
    CleverCoffeeWiFiManager* getWiFiManager() const noexcept {
        return wifiManager_;
    }

    /**
     * @brief Get MQTT manager
     */
    MQTTManager* getMQTTManager() const noexcept {
        return mqttManager_;
    }

    // === Hardware Component Access ===

    /**
     * @brief Get temperature sensor
     */
    TempSensor* getTempSensor() noexcept override;
    const TempSensor* getTempSensor() const noexcept override;

    /**
     * @brief Get temperature sensor (legacy method name)
     */
    TempSensor* getTemperatureSensor() const;

    /**
     * @brief Get water tank sensor
     */
    Switch* getWaterTankSensor() noexcept override;
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
    Relay* getHeaterRelay() noexcept override;
    const Relay* getHeaterRelay() const;

    /**
     * @brief Get pump relay
     */
    Relay* getPumpRelay() noexcept override;
    const Relay* getPumpRelay() const;

    /**
     * @brief Get valve relay
     */
    Relay* getValveRelay() noexcept override;
    const Relay* getValveRelay() const;

    /**
     * @brief Get status LED
     */
    LED* getStatusLed() noexcept override;
    const LED* getStatusLed() const noexcept override;

    /**
     * @brief Get status LED (legacy method name)
     */
    LED* getStatusLED() const;

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
     * @brief Set hot water state
     */
    void setHotWaterState(bool active) const;

    /**
     * @brief Set steam state
     */
    void setSteamState(bool active) const;

    /**
     * @brief Set backflush state
     */
    void setBackflushState(bool active) const;

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
    bool isWaterTankEmpty() const noexcept override;
    double getWeight() const noexcept override;
    void tareScale() noexcept override;
    void updateHardware() noexcept override;
    
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

    double getBrewSetpoint() const noexcept override;
    double getSteamSetpoint() const noexcept override;
    double getTargetBrewTime() const noexcept override;
    double getPreInfusionTime() const noexcept override;
    double getPidKp() const noexcept override;
    double getPidTn() const noexcept override;
    double getPidTv() const noexcept override;
    Config& getConfig() noexcept override;
    const Config& getConfig() const noexcept override;

    // === IStateManager Interface Implementation ===

    MachineStateId getCurrentStateId() const noexcept override;
    void transitionTo(MachineState& newState) override;
    unsigned long getStateStartTime() const noexcept override;

  private:
    // System context
    CleverCoffee::SystemContext& systemContext_;

    // Manager references
    DisplayManager*          displayManager_;
    CleverCoffee::HardwareManager*         hardwareManager_;
    SensorManager*           sensorManager_;
    CleverCoffeeWiFiManager* wifiManager_;
    MQTTManager*             mqttManager_;

    // State timing
    std::chrono::steady_clock::time_point stateEntryTime_; ///< Time when current state was entered
};
