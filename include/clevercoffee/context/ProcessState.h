/**
 * @file ProcessState.h
 * @brief Process state management for temperature control, PID, and brewing
 *
 * This class encapsulates all process-related state that was previously in SystemContext.
 * It provides a clean interface for temperature control, PID parameters, and brewing state.
 *
 * Design Pattern: Single Responsibility Principle
 * - ProcessState is responsible only for process control state
 * - Separated from SystemContext to reduce coupling
 * - Improves testability and maintainability
 */

#pragma once

namespace CleverCoffee {

/**
 * @class ProcessState
 * @brief Manages process control state (temperature, PID, brewing)
 *
 * This class encapsulates all state related to:
 * - Temperature control (current temperature, setpoint)
 * - PID control (output, parameters, enabled state)
 * - Brewing process (time, target time, PID disabled state)
 *
 * Example usage:
 * @code
 * ProcessState processState;
 * processState.setTemperature(95.5);
 * processState.setSetpoint(95.0);
 * double currentTemp = processState.temperature();
 * @endcode
 */
class ProcessState {
  public:
    ProcessState()  = default;
    ~ProcessState() = default;

    // Temperature
    double temperature() const noexcept {
        return temperature_;
    }
    void setTemperature(double temp) noexcept {
        temperature_ = temp;
    }

    // Setpoint
    double setpoint() const noexcept {
        return setpoint_;
    }
    void setSetpoint(double setpoint) noexcept {
        setpoint_ = setpoint;
    }

    // Steam setpoint
    double steamSetpoint() const noexcept {
        return steamSetpointValue_;
    }
    void setSteamSetpoint(double setpoint) noexcept {
        steamSetpointValue_ = setpoint;
    }

    // PID Output
    double pidOutput() const noexcept {
        return pidOutput_;
    }
    void setPidOutput(double output) noexcept {
        pidOutput_ = output;
    }

    // PID Enabled
    bool pidEnabled() const noexcept {
        return pidEnabled_;
    }
    void setPidEnabled(bool enabled) noexcept {
        pidEnabled_ = enabled;
    }

    // Brew Time
    double currentBrewTime() const noexcept {
        return currBrewTime_;
    }
    void setCurrentBrewTime(double time) noexcept {
        currBrewTime_ = time;
    }

    // Starting Time
    long startingTime() const noexcept {
        return startingTime_;
    }
    void setStartingTime(long time) noexcept {
        startingTime_ = time;
    }

    // Target Brew Time
    double totalTargetBrewTime() const noexcept {
        return totalTargetBrewTime_;
    }
    void setTotalTargetBrewTime(double time) noexcept {
        totalTargetBrewTime_ = time;
    }

    // Brew PID Disabled
    bool brewPidDisabled() const noexcept {
        return brewPidDisabled_;
    }
    void setBrewPidDisabled(bool disabled) noexcept {
        brewPidDisabled_ = disabled;
    }

    // Previous Input (for PID derivative)
    double previousInput() const noexcept {
        return previousInput_;
    }
    void setPreviousInput(double input) noexcept {
        previousInput_ = input;
    }

    // PID Parameters - Aggressive (Brew Detection)
    double pidAggKi() const noexcept {
        return aggbKi_;
    }
    void setPidAggKi(double value) noexcept {
        aggbKi_ = value;
    }

    double pidAggKd() const noexcept {
        return aggbKd_;
    }
    void setPidAggKd(double value) noexcept {
        aggbKd_ = value;
    }

    // PID Parameters - Normal
    double pidKi() const noexcept {
        return aggKi_;
    }
    void setPidKi(double value) noexcept {
        aggKi_ = value;
    }

    double pidKd() const noexcept {
        return aggKd_;
    }
    void setPidKd(double value) noexcept {
        aggKd_ = value;
    }

    // PID Window Size
    int windowSize() const noexcept {
        return windowSize_;
    }
    void setWindowSize(int size) noexcept {
        windowSize_ = size;
    }

    // Pointer accessors for PID controller (backward compatibility)
    double* temperaturePtr() noexcept {
        return &temperature_;
    }
    double* pidOutputPtr() noexcept {
        return &pidOutput_;
    }
    double* setpointPtr() noexcept {
        return &setpoint_;
    }

  private:
    // Temperature control
    double temperature_        = 0.0;
    double setpoint_           = 95.0;
    double steamSetpointValue_ = 120.0;

    // PID control
    double pidOutput_     = 0.0;
    bool   pidEnabled_    = true;
    double previousInput_ = 0.0;
    double aggbKi_        = 0.0; // Aggressive (brew detection) Ki
    double aggbKd_        = 0.0; // Aggressive (brew detection) Kd
    double aggKi_         = 0.0; // Normal Ki
    double aggKd_         = 0.0; // Normal Kd
    int    windowSize_    = 1000;

    // Brewing process
    double currBrewTime_        = 0.0;
    long   startingTime_        = 0;
    double totalTargetBrewTime_ = 0.0;
    bool   brewPidDisabled_     = false;
};

} // namespace CleverCoffee
