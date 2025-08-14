/**
 * @file EepromErrorState.h
 * @brief EEPROM error state for configuration storage errors
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class EepromErrorState : public MachineState {
    public:
        EepromErrorState() = default;
        ~EepromErrorState() override = default;

        void onEntry(MachineStateContext& context) override;
        void onExit(MachineStateContext& context) override;
        void update(MachineStateContext& context) override;
        std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

        MachineStateId getStateId() const override {
            return MachineStateId::EEPROM_ERROR;
        }
        const char* getStateName() const override {
            return "EEPROM Error";
        }
};