/**
 * @file ErrorStates.h
 * @brief All error and safety states consolidated
 */

#pragma once

#include "../BaseState.h"
#include "../GlobalState.h"

/**
 * @brief Sensor error state - sensor malfunction detected
 */
class SensorErrorState : public BaseState<MachineStateId::SENSOR_ERROR, SensorErrorState> {
public:
    static constexpr const char* STATE_NAME = "Sensor Error";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;

private:
    unsigned long errorStartTime_ = 0;
    static constexpr unsigned int MAX_RECOVERY_ATTEMPTS = 3;
    unsigned int recoveryAttempts_ = 0;
};

/**
 * @brief Water tank empty state - water tank needs refilling
 */
class WaterTankEmptyState : public BaseState<MachineStateId::WATER_TANK_EMPTY, WaterTankEmptyState> {
public:
    static constexpr const char* STATE_NAME = "Water Tank Empty";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief EEPROM error state - configuration storage error
 */
class EepromErrorState : public BaseState<MachineStateId::EEPROM_ERROR, EepromErrorState> {
public:
    static constexpr const char* STATE_NAME = "EEPROM Error";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};