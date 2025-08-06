/**
 * @file SensorErrorState.h
 * @brief Sensor error state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class SensorErrorState : public MachineState {
    public:
        SensorErrorState() = default;
        ~SensorErrorState() override = default;

        void onEntry(MachineStateContext& context) override;
        void onExit(MachineStateContext& context) override;
        void update(MachineStateContext& context) override;
        std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

        int getStateId() const override {
            return MachineStateIds::SENSOR_ERROR;
        }
        const char* getStateName() const override {
            return "Sensor Error";
        }

    private:
        unsigned long errorStartTime_ = 0; ///< Time when error was first detected
};