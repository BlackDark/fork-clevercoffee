/**
 * @file ProcessController.h
 * @brief Process controller for PID, temperature, and brewing control
 */

#pragma once

#include <PID_v1.h>
#include <memory>

// Forward declarations
class DisplayManager;
class HardwareManager;
class SensorManager;
class MQTTManager;
class TempSensor;
class Scale;

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
         * @brief Constructor
         * @param displayManager Display manager instance
         * @param hardwareManager Hardware manager instance
         * @param sensorManager Sensor manager instance
         * @param mqttManager MQTT manager instance
         */
        ProcessController(DisplayManager* displayManager, HardwareManager* hardwareManager, SensorManager* sensorManager, MQTTManager* mqttManager);

        /**
         * @brief Destructor
         */
        ~ProcessController() = default;

        // Disable copy constructor and assignment operator
        ProcessController(const ProcessController&) = delete;
        ProcessController& operator=(const ProcessController&) = delete;

        // Enable move constructor and assignment operator
        ProcessController(ProcessController&&) = default;
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
        void updateProcessControl(int machineState, bool brewPidDisabled);

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
        void updatePIDState(int machineState);

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
        bool shouldPIDBeEnabled(int machineState, bool brewPidDisabled) const;

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
         * @brief Emergency stop - immediately disable PID and turn off heater
         */
        void emergencyStop();

        /**
         * @brief Test for emergency conditions (overtemperature, etc.)
         * @return true if emergency stop was triggered
         */
        bool testEmergencyConditions();

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
         * @param machineState Current machine state
         * @param brewPidDisabled Whether brew PID is currently disabled
         */
        void handleBrewPIDDelay(int machineState, bool brewPidDisabled);

        // Manager dependencies
        DisplayManager* displayManager_;
        HardwareManager* hardwareManager_;
        SensorManager* sensorManager_;
        MQTTManager* mqttManager_;

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

        // Temperature offset
        double brewTempOffset_; ///< Temperature offset for brewing

        // State tracking
        int lastMachineStatePid_; ///< Last machine state for PID logging
        bool initialized_;        ///< Whether controller is initialized

        // Timing
        unsigned long lastTempEvent_;     ///< Last temperature event timestamp
        unsigned long tempEventInterval_; ///< Temperature event interval
};