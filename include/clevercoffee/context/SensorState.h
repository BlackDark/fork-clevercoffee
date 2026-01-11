/**
 * @file SensorState.h
 * @brief Sensor state management for pressure, weight, scale, and switch readings
 * 
 * This class encapsulates all sensor-related state that was previously in SystemContext.
 * It provides a clean interface for sensor readings, scale operations, and switch states.
 * 
 * Design Pattern: Single Responsibility Principle
 * - SensorState is responsible only for sensor-related state
 * - Separated from SystemContext to reduce coupling
 * - Improves testability and maintainability
 */

#pragma once

#include <Arduino.h>  // For LOW, HIGH
#include "clevercoffee/types/GlobalTypes.h"  // For SwitchState

namespace CleverCoffee {

/**
 * @class SensorState
 * @brief Manages sensor-related state (pressure, weight, scale, switches)
 * 
 * This class encapsulates all state related to:
 * - Pressure sensor readings
 * - Weight/scale readings
 * - Scale operations (tare, calibration)
 * - Switch states (brew, hot water, power, steam)
 * - Pump timing
 * - Pressure filter state
 * 
 * Example usage:
 * @code
 * SensorState sensorState;
 * sensorState.setInputPressure(9.5f);
 * sensorState.setCurrBrewWeight(250.0);
 * float pressure = sensorState.inputPressure();
 * @endcode
 */
class SensorState {
public:
    SensorState() = default;
    ~SensorState() = default;

    // Pressure
    float inputPressure() const noexcept { return inputPressure_; }
    void setInputPressure(float pressure) noexcept { inputPressure_ = pressure; }

    float inputPressureFilter() const noexcept { return inputPressureFilter_; }
    void setInputPressureFilter(float pressure) noexcept { inputPressureFilter_ = pressure; }

    // Weight
    double currBrewWeight() const noexcept { return currBrewWeight_; }
    void setCurrBrewWeight(double weight) noexcept { currBrewWeight_ = weight; }

    double currReadingWeight() const noexcept { return currReadingWeight_; }
    void setCurrReadingWeight(double weight) noexcept { currReadingWeight_ = weight; }

    double preBrewWeight() const noexcept { return preBrewWeight_; }
    void setPreBrewWeight(double weight) noexcept { preBrewWeight_ = weight; }

    float lastValidWeight() const noexcept { return lastValidWeight_; }
    void setLastValidWeight(float weight) noexcept { lastValidWeight_ = weight; }

    // Scale Operations
    bool scaleFailure() const noexcept { return scaleFailure_; }
    void setScaleFailure(bool failure) noexcept { scaleFailure_ = failure; }

    bool scaleTareOn() const noexcept { return scaleTareOn_; }
    void setScaleTareOn(bool on) noexcept { scaleTareOn_ = on; }

    bool scaleCalibrationOn() const noexcept { return scaleCalibrationOn_; }
    void setScaleCalibrationOn(bool on) noexcept { scaleCalibrationOn_ = on; }

    // Scale Connection State
    bool scaleConnectionLost() const noexcept { return scaleConnectionLost_; }
    void setScaleConnectionLost(bool lost) noexcept { scaleConnectionLost_ = lost; }

    unsigned long lastScaleConnectionCheck() const noexcept { return lastScaleConnectionCheck_; }
    void setLastScaleConnectionCheck(unsigned long time) noexcept { lastScaleConnectionCheck_ = time; }

    unsigned long scaleConnectionFailureTime() const noexcept { return scaleConnectionFailureTime_; }
    void setScaleConnectionFailureTime(unsigned long time) noexcept { scaleConnectionFailureTime_ = time; }

    // Scale Error Recovery
    int scaleReadErrorCount() const noexcept { return scaleReadErrorCount_; }
    void setScaleReadErrorCount(int count) noexcept { scaleReadErrorCount_ = count; }

    int scaleMaxRetries() const noexcept { return scaleMaxRetries_; }
    void setScaleMaxRetries(int retries) noexcept { scaleMaxRetries_ = retries; }

    unsigned long lastScaleErrorTime() const noexcept { return lastScaleErrorTime_; }
    void setLastScaleErrorTime(unsigned long time) noexcept { lastScaleErrorTime_ = time; }

    unsigned long scaleErrorCooldownMs() const noexcept { return scaleErrorCooldownMs_; }
    void setScaleErrorCooldownMs(unsigned long cooldown) noexcept { scaleErrorCooldownMs_ = cooldown; }

    bool scaleInErrorRecovery() const noexcept { return scaleInErrorRecovery_; }
    void setScaleInErrorRecovery(bool inRecovery) noexcept { scaleInErrorRecovery_ = inRecovery; }

    // Auto Tare
    bool autoTareInProgress() const noexcept { return autoTareInProgress_; }
    void setAutoTareInProgress(bool inProgress) noexcept { autoTareInProgress_ = inProgress; }

    unsigned long autoTareStartTime() const noexcept { return autoTareStartTime_; }
    void setAutoTareStartTime(unsigned long time) noexcept { autoTareStartTime_ = time; }

    // Brew by Weight Fallback
    bool brewByWeightFallbackActive() const noexcept { return brewByWeightFallbackActive_; }
    void setBrewByWeightFallbackActive(bool active) noexcept { brewByWeightFallbackActive_ = active; }

    // Pressure Filter State
    float inX() const noexcept { return inX_; }
    void setInX(float value) noexcept { inX_ = value; }

    float inY() const noexcept { return inY_; }
    void setInY(float value) noexcept { inY_ = value; }

    float inOld() const noexcept { return inOld_; }
    void setInOld(float value) noexcept { inOld_ = value; }

    float inSum() const noexcept { return inSum_; }
    void setInSum(float value) noexcept { inSum_ = value; }

    // Pump Timing
    double currPumpOnTime() const noexcept { return currPumpOnTime_; }
    void setCurrPumpOnTime(double time) noexcept { currPumpOnTime_ = time; }

    unsigned long pumpStartingTime() const noexcept { return pumpStartingTime_; }
    void setPumpStartingTime(unsigned long time) noexcept { pumpStartingTime_ = time; }

    // Water Tank Check
    int waterTankCheckConsecutiveReads() const noexcept { return waterTankCheckConsecutiveReads_; }
    void setWaterTankCheckConsecutiveReads(int reads) noexcept { waterTankCheckConsecutiveReads_ = reads; }

    // Switch States
    SwitchState currBrewSwitchState() const noexcept { return currBrewSwitchState_; }
    void setCurrBrewSwitchState(SwitchState state) noexcept { currBrewSwitchState_ = state; }

    uint8_t brewSwitchReading() const noexcept { return brewSwitchReading_; }
    void setBrewSwitchReading(uint8_t reading) noexcept { brewSwitchReading_ = reading; }

    uint8_t currReadingBrewSwitch() const noexcept { return currReadingBrewSwitch_; }
    void setCurrReadingBrewSwitch(uint8_t reading) noexcept { currReadingBrewSwitch_ = reading; }

    bool brewSwitchWasOff() const noexcept { return brewSwitchWasOff_; }
    void setBrewSwitchWasOff(bool wasOff) noexcept { brewSwitchWasOff_ = wasOff; }

    SwitchState currHotWaterSwitchState() const noexcept { return currHotWaterSwitchState_; }
    void setCurrHotWaterSwitchState(SwitchState state) noexcept { currHotWaterSwitchState_ = state; }

    uint8_t hotWaterSwitchReading() const noexcept { return hotWaterSwitchReading_; }
    void setHotWaterSwitchReading(uint8_t reading) noexcept { hotWaterSwitchReading_ = reading; }

    uint8_t currReadingHotWaterSwitch() const noexcept { return currReadingHotWaterSwitch_; }
    void setCurrReadingHotWaterSwitch(uint8_t reading) noexcept { currReadingHotWaterSwitch_ = reading; }

    uint8_t currStateSteamSwitch() const noexcept { return currStateSteamSwitch_; }
    void setCurrStateSteamSwitch(uint8_t state) noexcept { currStateSteamSwitch_ = state; }

    bool currStatePowerSwitchPressed() const noexcept { return currStatePowerSwitchPressed_; }
    void setCurrStatePowerSwitchPressed(bool pressed) noexcept { currStatePowerSwitchPressed_ = pressed; }

    bool lastPowerSwitchPressed() const noexcept { return lastPowerSwitchPressed_; }
    void setLastPowerSwitchPressed(bool pressed) noexcept { lastPowerSwitchPressed_ = pressed; }

    // System Initialization Timing
    unsigned long systemInitializedTime() const noexcept { return systemInitializedTime_; }
    void setSystemInitializedTime(unsigned long time) noexcept { systemInitializedTime_ = time; }

    unsigned long firstSwitchPressTime() const noexcept { return firstSwitchPressTime_; }
    void setFirstSwitchPressTime(unsigned long time) noexcept { firstSwitchPressTime_ = time; }

    bool trackingPressTime() const noexcept { return trackingPressTime_; }
    void setTrackingPressTime(bool tracking) noexcept { trackingPressTime_ = tracking; }

    // Shot Timer
    int shottimerCounter() const noexcept { return shottimerCounter_; }
    void setShottimerCounter(int counter) noexcept { shottimerCounter_ = counter; }

private:
    // Pressure
    float inputPressure_ = 0.0f;
    float inputPressureFilter_ = 0.0f;

    // Weight
    double currBrewWeight_ = 0.0;
    double currReadingWeight_ = 0.0;
    float preBrewWeight_ = 0.0f;
    float lastValidWeight_ = 0.0f;

    // Scale Operations
    bool scaleFailure_ = false;
    bool scaleTareOn_ = false;
    bool scaleCalibrationOn_ = false;

    // Scale Connection State
    bool scaleConnectionLost_ = false;
    unsigned long lastScaleConnectionCheck_ = 0;
    unsigned long scaleConnectionFailureTime_ = 0;

    // Scale Error Recovery
    int scaleReadErrorCount_ = 0;
    int scaleMaxRetries_ = 5;
    unsigned long lastScaleErrorTime_ = 0;
    unsigned long scaleErrorCooldownMs_ = 1000;
    bool scaleInErrorRecovery_ = false;

    // Auto Tare
    bool autoTareInProgress_ = false;
    unsigned long autoTareStartTime_ = 0;

    // Brew by Weight Fallback
    bool brewByWeightFallbackActive_ = false;

    // Pressure Filter State
    float inX_ = 0.0f;
    float inY_ = 0.0f;
    float inOld_ = 0.0f;
    float inSum_ = 0.0f;

    // Pump Timing
    double currPumpOnTime_ = 0.0;
    unsigned long pumpStartingTime_ = 0;

    // Water Tank Check
    int waterTankCheckConsecutiveReads_ = 0;

    // Switch States
    SwitchState currBrewSwitchState_ = SwitchState::IDLE;
    uint8_t brewSwitchReading_ = LOW;
    uint8_t currReadingBrewSwitch_ = LOW;
    bool brewSwitchWasOff_ = false;

    SwitchState currHotWaterSwitchState_ = SwitchState::IDLE;
    uint8_t hotWaterSwitchReading_ = LOW;
    uint8_t currReadingHotWaterSwitch_ = LOW;

    uint8_t currStateSteamSwitch_ = 0;
    bool currStatePowerSwitchPressed_ = false;
    bool lastPowerSwitchPressed_ = false;

    // System Initialization Timing
    unsigned long systemInitializedTime_ = 0;
    unsigned long firstSwitchPressTime_ = 0;
    bool trackingPressTime_ = false;

    // Shot Timer
    int shottimerCounter_ = 10;
};

} // namespace CleverCoffee
