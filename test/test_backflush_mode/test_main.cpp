/**
 * @file test_main.cpp
 * @brief Unit tests for backflush mode logic
 */

#include <gtest/gtest.h>

#include "../test_support.h"
#include "clevercoffee/backflush/BackflushModeLogic.h"

using CleverCoffee::Backflush::CycleAdvanceEffect;
using CleverCoffee::Backflush::ModeChangeEffect;
using CleverCoffee::Backflush::ModeChangeInput;
using CleverCoffee::Backflush::resolveCycleAdvance;
using CleverCoffee::Backflush::resolveModeChange;

TEST(BackflushModeLogicTest, NoChangeWhenAlreadyActive) {
    EXPECT_EQ(ModeChangeEffect::None, resolveModeChange({true, true, 5}));
}

TEST(BackflushModeLogicTest, NoChangeWhenAlreadyInactive) {
    EXPECT_EQ(ModeChangeEffect::None, resolveModeChange({false, false, 5}));
}

TEST(BackflushModeLogicTest, EnableWhenInactiveAndCyclesConfigured) {
    EXPECT_EQ(ModeChangeEffect::Enable, resolveModeChange({false, true, 5}));
}

TEST(BackflushModeLogicTest, RejectEnableWhenCyclesZero) {
    EXPECT_EQ(ModeChangeEffect::RejectedInvalidCycles, resolveModeChange({false, true, 0}));
}

TEST(BackflushModeLogicTest, RejectEnableWhenCyclesNegative) {
    EXPECT_EQ(ModeChangeEffect::RejectedInvalidCycles, resolveModeChange({false, true, -1}));
}

TEST(BackflushModeLogicTest, DisableWhenActive) {
    EXPECT_EQ(ModeChangeEffect::Disable, resolveModeChange({true, false, 5}));
}

TEST(BackflushCycleLogicTest, StartsNextCycleWhenBelowConfigured) {
    EXPECT_EQ(CycleAdvanceEffect::StartNextCycle, resolveCycleAdvance(1, 5));
    EXPECT_EQ(CycleAdvanceEffect::StartNextCycle, resolveCycleAdvance(4, 5));
}

TEST(BackflushCycleLogicTest, CompletesWhenConfiguredCyclesReached) {
    EXPECT_EQ(CycleAdvanceEffect::CompleteAllCycles, resolveCycleAdvance(5, 5));
}

TEST(BackflushCycleLogicTest, CompletesWhenCurrentExceedsConfigured) {
    EXPECT_EQ(CycleAdvanceEffect::CompleteAllCycles, resolveCycleAdvance(6, 5));
}

TEST(BackflushModeLogicTest, RejectedEnableMapsToApplyFailure) {
    EXPECT_EQ(ModeChangeEffect::RejectedInvalidCycles, resolveModeChange({false, true, 0}));
}
