/**
 * @file test_main.cpp
 * @brief Unit tests for TSIC temperature sensor glitch rejection and stabilisation
 *
 * Regression coverage for two field-observed bugs:
 * 1. Out-of-range glitches (e.g. -2.9 °C) were passed through as valid readings,
 *    tripping the emergency-stop invalid-reading path on a single sample.
 * 2. tryGetValue() seeded the previous reading with 0.0, so the TSIC stabilisation
 *    never latched — flooding the log with "Temperature not stable".
 */

#include <gtest/gtest.h>

#include "../test_support.h"

#include "clevercoffee/hardware/tempsensors/TempSensorTSIC.h"

#include "../mocks/ConfigStubs.cpp"
#include "../../src/Logger.cpp"
#include "../../src/hardware/tempsensors/TempSensorTSIC.cpp"

using namespace CleverCoffee;

namespace {

// Drive a single read cycle through the ISensor interface.
Expected<double, Error> readOnce(TempSensorTSIC& sensor) {
    sensor.startRead();
    return sensor.tryGetValue();
}

} // namespace

class TempSensorTSICTest : public ::testing::Test {
  protected:
    void SetUp() override { ZACwire::reset(); }
};

// A physically impossible reading must not surface as a valid value — it must be
// reported as not-ready so the caller keeps the last good cached temperature and
// the emergency-stop logic never sees the garbage value.
TEST_F(TempSensorTSICTest, NegativeGlitchIsRejected) {
    TempSensorTSIC sensor(4);

    ZACwire::setNext(-2.9f);
    auto result = readOnce(sensor);

    EXPECT_FALSE(result.hasValue());
}

// Values above the sensor's physical range are likewise rejected.
TEST_F(TempSensorTSICTest, OverRangeGlitchIsRejected) {
    TempSensorTSIC sensor(4);

    ZACwire::setNext(250.0f);
    auto result = readOnce(sensor);

    EXPECT_FALSE(result.hasValue());
}

// Sensor error codes continue to be treated as failed reads.
TEST_F(TempSensorTSICTest, SensorErrorCodesAreRejected) {
    TempSensorTSIC sensor(4);

    ZACwire::setNext(222.0f); // read failed
    EXPECT_FALSE(readOnce(sensor).hasValue());

    ZACwire::setNext(221.0f); // not connected
    EXPECT_FALSE(readOnce(sensor).hasValue());
}

// A normal in-range reading is returned unchanged.
TEST_F(TempSensorTSICTest, ValidReadingIsReturned) {
    TempSensorTSIC sensor(4);

    ZACwire::setNext(94.0f);
    auto result = readOnce(sensor);

    ASSERT_TRUE(result.hasValue());
    EXPECT_NEAR(result.value(), 94.0, 0.01);
}

// A glitch between two good readings must not corrupt the cached value: the bad
// sample is dropped and the next good sample reads correctly.
TEST_F(TempSensorTSICTest, GlitchBetweenGoodReadingsDoesNotPropagate) {
    TempSensorTSIC sensor(4);

    ZACwire::setNext(95.0f);
    auto first = readOnce(sensor);
    ASSERT_TRUE(first.hasValue());
    EXPECT_NEAR(first.value(), 95.0, 0.01);

    ZACwire::setNext(-2.9f);
    EXPECT_FALSE(readOnce(sensor).hasValue());

    ZACwire::setNext(95.2f);
    auto third = readOnce(sensor);
    ASSERT_TRUE(third.hasValue());
    EXPECT_NEAR(third.value(), 95.2, 0.01);
}
