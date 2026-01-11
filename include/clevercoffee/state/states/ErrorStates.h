/**
 * @file ErrorStates.h
 * @brief Error-related state classes
 */

#pragma once

#include "clevercoffee/state/BaseState.h"
#include <optional>

// Error States
class SensorErrorState : public BaseState<MachineStateId::SENSOR_ERROR, SensorErrorState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    void          onExitImpl(MachineStateContext& context) override;
    std::optional<MachineStateId> checkSpecificTransitions(MachineStateContext& context) override;

  private:
    static constexpr unsigned int MAX_RECOVERY_ATTEMPTS = 3;
    unsigned long                 errorStartTime_   = 0;  // Fresh on each entry (new instance)
    unsigned int                  recoveryAttempts_ = 0;  // Fresh on each entry (new instance)
};

class WaterTankEmptyState : public BaseState<MachineStateId::WATER_TANK_EMPTY, WaterTankEmptyState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    std::optional<MachineStateId> checkSpecificTransitions(MachineStateContext& context) override;
};

class EepromErrorState : public BaseState<MachineStateId::EEPROM_ERROR, EepromErrorState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    void          onExitImpl(MachineStateContext& context) override;
    std::optional<MachineStateId> checkSpecificTransitions(MachineStateContext& context) override;

  private:
    unsigned long errorStartTime_ = 0;  // Fresh on each entry (new instance)
};