/**
 * @file test_main.cpp
 * @brief Unit tests for power-on behaviour resolver
 */

#include <gtest/gtest.h>

#include "../test_support.h"
#include "clevercoffee/core/PowerOnBehaviour.h"

using CleverCoffee::Core::resolvePowerOnBehaviour;

TEST(PowerOnBehaviourTest, MomentaryStandbyLeavesPidConfigUntouchedInResolver) {
    const auto r = resolvePowerOnBehaviour(true,
                                           Hardware::SwitchType::MOMENTARY,
                                           false,
                                           Process::PowerOnBehaviour::STANDBY,
                                           true);
    EXPECT_EQ(r.state, MachineStateId::STANDBY);
    EXPECT_FALSE(r.runtimePid);
    EXPECT_FALSE(r.resetStandbyTimer);
    EXPECT_FALSE(r.persistPid);
}

TEST(PowerOnBehaviourTest, MomentaryHeatDoesNotRequirePidEnabled) {
    const auto r = resolvePowerOnBehaviour(
        true, Hardware::SwitchType::MOMENTARY, false, Process::PowerOnBehaviour::HEAT, false);
    EXPECT_EQ(r.state, MachineStateId::PID_NORMAL);
    EXPECT_TRUE(r.runtimePid);
    EXPECT_TRUE(r.resetStandbyTimer);
    EXPECT_FALSE(r.persistPid);
}

TEST(PowerOnBehaviourTest, MomentaryRestoreFollowsPidEnabled) {
    const auto on = resolvePowerOnBehaviour(
        true, Hardware::SwitchType::MOMENTARY, false, Process::PowerOnBehaviour::RESTORE, true);
    EXPECT_EQ(on.state, MachineStateId::PID_NORMAL);
    EXPECT_TRUE(on.runtimePid);
    EXPECT_TRUE(on.resetStandbyTimer);
    EXPECT_FALSE(on.persistPid);

    const auto off = resolvePowerOnBehaviour(
        true, Hardware::SwitchType::MOMENTARY, false, Process::PowerOnBehaviour::RESTORE, false);
    EXPECT_EQ(off.state, MachineStateId::STANDBY);
    EXPECT_FALSE(off.runtimePid);
    EXPECT_FALSE(off.resetStandbyTimer);
    EXPECT_FALSE(off.persistPid);
}

TEST(PowerOnBehaviourTest, NoSwitchMatchesMomentary) {
    const auto r = resolvePowerOnBehaviour(
        false, Hardware::SwitchType::MOMENTARY, true, Process::PowerOnBehaviour::HEAT, false);
    EXPECT_EQ(r.state, MachineStateId::PID_NORMAL);
    EXPECT_TRUE(r.runtimePid);
    EXPECT_FALSE(r.persistPid);
}

TEST(PowerOnBehaviourTest, ToggleIgnoresEnum) {
    for (const auto behaviour : {Process::PowerOnBehaviour::STANDBY,
                                 Process::PowerOnBehaviour::HEAT,
                                 Process::PowerOnBehaviour::RESTORE}) {
        const auto on = resolvePowerOnBehaviour(true, Hardware::SwitchType::TOGGLE, true, behaviour, false);
        EXPECT_EQ(on.state, MachineStateId::PID_NORMAL) << static_cast<int>(behaviour);
        EXPECT_TRUE(on.runtimePid);
        EXPECT_TRUE(on.persistPid);

        const auto off = resolvePowerOnBehaviour(true, Hardware::SwitchType::TOGGLE, false, behaviour, true);
        EXPECT_EQ(off.state, MachineStateId::PID_DISABLED) << static_cast<int>(behaviour);
        EXPECT_FALSE(off.runtimePid);
        EXPECT_FALSE(off.resetStandbyTimer);
        EXPECT_FALSE(off.persistPid);
    }
}

TEST(PowerOnBehaviourTest, InvalidEnumDefaultsToStandby) {
    const auto r = resolvePowerOnBehaviour(false,
                                           Hardware::SwitchType::MOMENTARY,
                                           false,
                                           static_cast<Process::PowerOnBehaviour>(99),
                                           true);
    EXPECT_EQ(r.state, MachineStateId::STANDBY);
    EXPECT_FALSE(r.runtimePid);
    EXPECT_FALSE(r.persistPid);
}
