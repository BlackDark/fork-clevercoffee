/**
 * @file test_main.cpp
 * @brief Unit tests for display tolerance helpers (pure overloads)
 */

#include <gtest/gtest.h>

#include "clevercoffee/constants/Temperature.h"
#include "clevercoffee/display/displayHelpers.h"
#include "clevercoffee/state/MachineStateIds.h"

namespace {

using CleverCoffee::Display::isNearSetpointForDisplay;
using CleverCoffee::Display::isNearSetpointForStatusLed;
using CleverCoffee::Temperature::HEATING_LOGO_THRESHOLD_C;

} // namespace

TEST(DisplayHelpersTest, NearSetpointDisplayUsesStrictLessThan) {
    constexpr double delta = 0.3;
    EXPECT_TRUE(isNearSetpointForDisplay(94.71, 95.0, delta));
    EXPECT_FALSE(isNearSetpointForDisplay(94.69, 95.0, delta));
    EXPECT_TRUE(isNearSetpointForDisplay(95.0, 95.0, delta));
}

TEST(DisplayHelpersTest, NearSetpointStatusLedUsesLessOrEqual) {
    constexpr double delta = 0.3;
    EXPECT_TRUE(isNearSetpointForStatusLed(95.0, 95.0, delta));
    EXPECT_TRUE(isNearSetpointForStatusLed(94.7, 95.0, delta));
    EXPECT_FALSE(isNearSetpointForStatusLed(94.69, 95.0, delta));
}

TEST(DisplayHelpersTest, StatusLedUsesSteamToleranceInSteamState) {
    constexpr double steamTolerance = CleverCoffee::Temperature::TEMP_TOLERANCE_STEAM_C;
    EXPECT_TRUE(isNearSetpointForStatusLed(90.0, 95.0, steamTolerance));
    EXPECT_FALSE(isNearSetpointForStatusLed(89.9, 95.0, steamTolerance));
}

TEST(DisplayHelpersTest, HeatingLogoThresholdConstant) {
    EXPECT_FLOAT_EQ(5.0f, HEATING_LOGO_THRESHOLD_C);
}
