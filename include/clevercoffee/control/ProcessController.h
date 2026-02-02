/**
 * @file ProcessController.h
 * @brief Process controller for PID, temperature, and brewing control
 */

#pragma once

#include "clevercoffee/control/EmergencyStopManager.h"
#include "clevercoffee/state/MachineStateIds.h"

#include <PID_v1.h>
#include <memory>

// Forward declarations
class IDisplayManager;
class IMQTTManager;
class TempSensor;
class Scale;
class Config;

namespace CleverCoffee {
class IHardwareContext;
class SystemContext;
} // namespace CleverCoffee

/**
 * @class ProcessController
 * @brief Central controller for process control (PID, brewing, steaming)
 *
 * This class manages all process control aspects including:
 * - PID temperature control with different tuning modes
 * - Brewing process control and timing
 * - Steam process control
 * - Temperature sensor management and updates
 * - Process state management and safety controls
 *
 * Key responsibilities:
 * - Maintain PID controller and compute outputs
 * - Handle temperature updates from sensors
 * - Manage PID tuning for different operating modes
 * - Control brewing and steaming processes
 * - Provide safety controls and emergency handling
 */
class ProcessController {
  public:
    /**
     * @param config Configuration instance
     * @param systemContext System context for sensor coordinator access (REQUIRED)
     * @param hardwareManager Hardware manager instance (REQUIRED - CRITICAL component)
     * @param displayManager Display manager instance (OPTIONAL)
     * @param displayManager Display manager (REQUIRED - always exists)
     * @param mqttManager MQTT manager (REQUIRED - always exists)
     */
    ProcessController(const Config&                   config,
                      CleverCoffee::SystemContext&    systemContext,
                      CleverCoffee::IHardwareContext& hardwareManager,
                      IDisplayManager&                displayManager,
                      IMQTTManager&                   mqttManager);

    /**
     * @brief Destructor
     */
    ~ProcessController() = default;

    // Disable copy constructor and assignment operator
    ProcessController(const ProcessController&)            = delete;
    ProcessController& operator=(const ProcessController&) = delete;

    // Enable move constructor and assignment operator
    ProcessController(ProcessController&&)            = default;
    ProcessController& operator=(ProcessController&&) = default;

    /**
     * @brief Initialize process controller
     * @return true if initialization successful
     */
    bool initialize();

    /**
     * @brief Main process control update - call from main loop
     *
     * This method should be called regularly from the main loop.
     * It handles temperature updates, PID computation, and process control.
     */
    void update();

    /**
     * @brief Complete process control cycle including state management
     * @param machineState Current machine state
     * @param brewPidDisabled Whether brew PID is disabled
     *
     * This method handles the complete PID control cycle including:
     * - Temperature updates from sensors
     * - Emergency condition testing
     * - PID computation
     * - Setpoint management based on steam mode
     * - PID state management based on machine state
     * - Debug logging
     */
    void updateProcessControl(MachineStateId machineState);

    /**
     * @brief Update temperature readings from sensors
     *
     * Reads temperature from sensors and applies appropriate offsets
     * based on current machine state (brew vs steam mode).
     */
    void updateTemperature();

    /**
     * @brief Compute PID output
     *
     * Runs PID computation and updates pidOutput variable.
     * Should be called after temperature update.
     */
    void computePID();

    /**
     * @brief Update PID state based on machine state
     * @param machineState Current machine state
     *
     * Enables/disables PID and sets appropriate tuning parameters
     * based on the current machine state.
     */
    void updatePIDState(MachineStateId machineState);

    /**
     * @brief Set PID tuning parameters for normal operation
     * @param usePonM Whether to use Proportional on Measurement mode
     */
    void setPIDTunings(bool usePonM);

    /**
     * @brief Set PID tuning parameters for brew detection mode
     */
    void setBrewDetectionPIDTunings();

    /**
     * @brief Set PID tuning parameters for steam mode
     */
    void setSteamPIDTunings();

    /**
     * @brief Update setpoint based on current mode
     * @param steamActive Whether steam mode is active
     */
    void updateSetpoint(bool steamActive);

    /**
     * @brief Check if PID should be enabled for current state
     * @param machineState Current machine state
     * @param brewPidDisabled Whether brew PID is disabled
     * @return true if PID should be enabled
     */
    bool shouldPIDBeEnabled(MachineStateId machineState) const;

    /**
     * @brief Get current temperature
     * @return Current temperature in Celsius
     */
    double getCurrentTemperature() const {
        return temperature_;
    }

    /**
     * @brief Get current PID output
     * @return Current PID output (0-1023)
     */
    double getPIDOutput() const {
        return pidOutput_;
    }

    /**
     * @brief Get current setpoint
     * @return Current temperature setpoint in Celsius
     */
    double getSetpoint() const {
        return setpoint_;
    }

    /**
     * @brief Check if PID is currently enabled
     * @return true if PID is in automatic mode
     */
    bool isPIDEnabled() const;

    /**
     * @brief Enable or disable PID control
     * @param enabled Whether to enable PID
     */
    void setPIDEnabled(bool enabled);

    /**
     * @brief Get current brew time in milliseconds
     * @return Current brew time (set externally by state machines)
     */
    double getCurrBrewTime() const;

    /**
     * @brief Set current brew time in milliseconds
     * @param brewTime Brew time to set
     */
    void setCurrBrewTime(double brewTime);

    /**
     * @brief Get total target brew time in milliseconds
     * @return Target brew time
     */
    double getTotalTargetBrewTime() const;

    /**
     * @brief Set total target brew time in milliseconds
     * @param brewTime Target brew time
     */
    void setTotalTargetBrewTime(double brewTime);

    /**
     * @brief Get brew PID disabled flag
     * @return true if brew PID is disabled
     */
    bool isBrewPidDisabled() const;

    /**
     * @brief Set brew PID disabled flag
     * @param disabled Whether brew PID should be disabled
     */
    void setBrewPidDisabled(bool disabled);

    /**
     * @brief Emergency stop - immediately disable PID and turn off heater
     */
    void emergencyStop();

    /**
     * @brief Safe shutdown - completely shutdown all machine operations
     *
     * This method performs a comprehensive shutdown of all machine operations:
     * - Disables PID control
     * - Turns off all relays (heater, pump, valve)
     * - Resets all brewing states
     * - Resets manual flush states
     * - Resets backflush states
     * - Disables steam mode if active
     * - Resets hot water state
     */
    void performSafeShutdown();

    /**
     * @brief Test for emergency conditions (overtemperature, etc.)
     * @return true if emergency stop was triggered
     */
    bool testEmergencyConditions();

    /**
     * @brief Check if emergency can be cleared
     * @param temperature Current temperature reading
     * @return true if emergency can be cleared
     */
    bool isEmergencyCleared(double temperature) const;

    /**
     * @brief Get PID normal mode Ki parameter
     * @return Current Ki value
     */
    double getAggKi() const {
        return aggKi_;
    }

    /**
     * @brief Get PID normal mode Kd parameter
     * @return Current Kd value
     */
    double getAggKd() const {
        return aggKd_;
    }

    /**
     * @brief Get PID normal mode Kp parameter
     * @return Current Kp value
     */
    double getAggKp() const {
        return aggKp_;
    }

    /**
     * @brief Get PID window size (0-1000 ms typically)
     * @return Window size in milliseconds
     */
    int getWindowSize() const {
        return windowSize_;
    }

    /**
     * @brief Get brew detection PID Ki parameter
     * @return Current brew detection Ki value
     */
    double getAggbKi() const {
        return aggbKi_;
    }

    /**
     * @brief Get brew detection PID Kd parameter
     * @return Current brew detection Kd value
     */
    double getAggbKd() const {
        return aggbKd_;
    }

  private:
    /**
     * @brief Update process control debug logging
     */
    void updateDebugLogging();

    /**
     * @brief Calculate derived PID parameters (Ki, Kd) from Tn, Tv
     */
    void calculatePIDParameters();

    /**
     * @brief Calculate brew detection PID parameters
     */
    void calculateBrewDetectionPIDParameters();

    /**
     * @brief Handle brew PID delay logic during brewing
     *
     * Manages PID state during brew process:
     * - Disables PID during initial delay period (first N seconds) to prevent overshoot
     * - Re-enables PID after delay period with appropriate tunings
     * - Re-enables PID if brew is aborted during delay period
     *
     * @param machineState Current machine state
     */
    void handleBrewPIDDelay(MachineStateId machineState);

  private:
    /**
     * @brief Disable PID during brew delay period
     * Called when brew is in the initial delay period
     */
    void disablePIDForBrewDelay() noexcept;

    /**
     * @brief Re-enable PID after brew delay period
     * Called when brew time exceeds the delay period
     */
    void enablePIDAfterBrewDelay() noexcept;

    /**
     * @brief Re-enable PID when brew is aborted
     * Called when brew state exits but PID is still disabled
     */
    void reEnablePIDAfterBrewAbort() noexcept;

  public:
    // Configuration reference (not owned)
    const Config& config_;

    // System context reference (not owned)
    CleverCoffee::SystemContext& systemContext_;

    // Manager dependencies - ALL REQUIRED
    CleverCoffee::IHardwareContext& hardwareManager_; // REQUIRED - CRITICAL component
    IDisplayManager&                displayManager_;  // REQUIRED - always exists
    IMQTTManager&                   mqttManager_;     // REQUIRED - always exists

    // PID controller
    std::unique_ptr<PID> pidController_;

    // Process variables
    double temperature_; ///< Current temperature reading
    double pidOutput_;   ///< Current PID output (0-1023)
    double setpoint_;    ///< Current temperature setpoint

    // PID parameters (normal mode)
    double aggKp_, aggKi_, aggKd_; ///< Aggressive PID parameters
    double aggTn_, aggTv_;         ///< Time constants for Ki/Kd calculation
    double aggIMax_;               ///< Integrator maximum value

    // PID parameters (brew detection mode)
    double aggbKp_, aggbKi_, aggbKd_; ///< Brew detection PID parameters
    double aggbTn_, aggbTv_;          ///< Brew detection time constants

    // Steam parameters
    double steamKp_; ///< Steam mode proportional gain

    // Setpoint values
    double brewSetpoint_;  ///< Target temperature for brewing
    double steamSetpoint_; ///< Target temperature for steam

    // Brewing process state
    double currBrewTime_;        ///< Current brew time in milliseconds
    double totalTargetBrewTime_; ///< Target brew time in milliseconds
    bool   brewPidDisabled_;     ///< Whether brew PID is disabled

    // Temperature offset
    double brewTempOffset_; ///< Temperature offset for brewing

    // PWM control
    int windowSize_ = 1000; ///< PID window/period size in milliseconds

    // Emergency stop management (centralized)
    std::unique_ptr<CleverCoffee::EmergencyStopManager> emergencyStopManager_;

    // State tracking
    MachineStateId lastMachineStatePid_; ///< Last machine state for PID logging
    bool           initialized_;         ///< Whether controller is initialized

    // Timing
    unsigned long lastTempEvent_;     ///< Last temperature event timestamp
    unsigned long tempEventInterval_; ///< Temperature event interval
};
