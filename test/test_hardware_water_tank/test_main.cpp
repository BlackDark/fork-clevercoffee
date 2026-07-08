/**
 * @file test_main.cpp
 * @brief Unit tests for HardwareManager water tank pump safety guards
 *
 * Compiles the production HardwareManager.cpp (plus the real Relay logic) so
 * the guards under test are the shipped implementation, not a stub. Pump state
 * is observed at the GPIO level through a spy GPIOPin that records the last
 * level written to each pin.
 */

#include <gtest/gtest.h>

#include <map>
#include <memory>

#include "../test_support.h"
#include "../mocks/ConfigStubs.cpp"
#include "../../src/Logger.cpp"

#include "clevercoffee/hardware/GPIOPin.h"
#include "clevercoffee/hardware/pinmapping.h"

// === GPIOPin spy: records the last level written to each pin ===
namespace TestPins {
inline std::map<int, bool> level;

inline void reset() {
    level.clear();
}
} // namespace TestPins

GPIOPin::GPIOPin(int pinNumber, Type type) : pin(pinNumber), pinType(type) {}
void GPIOPin::write(bool value) const noexcept {
    TestPins::level[pin] = value;
}
int GPIOPin::read() const noexcept {
    return 0;
}
GPIOPin::Type GPIOPin::getType() const noexcept {
    return pinType;
}
void GPIOPin::setType(Type newType) {
    pinType = newType;
}

// Arduino yield() is not provided by the native test shims
void yield() {}

// TempSensorDallas stubs — the DallasTemperature native shim is too minimal to
// compile the real implementation, and these tests never sample temperatures.
#include "clevercoffee/hardware/tempsensors/TempSensorDallas.h"
TempSensorDallas::TempSensorDallas(int /*GPIOPin*/) : oneWire_(nullptr), dallasSensor_(nullptr) {}
TempSensorDallas::~TempSensorDallas() = default;
bool TempSensorDallas::sample_temperature(double& /*temperature*/) const {
    return false;
}

// === Production code under test ===
#include "../../src/hardware/IOSwitch.cpp"
#include "../../src/hardware/Relay.cpp"
#include "../../src/hardware/StandardLED.cpp"
#include "../../src/hardware/tempsensors/TempSensorTSIC.cpp"
#include "../../src/hardware/HardwareManager.cpp"

using namespace CleverCoffee;

class HardwareWaterTankTest : public ::testing::Test {
  protected:
    void SetUp() override {
        g_test_millis = 0;
        TestPins::reset();
        // Deterministic relay semantics: HIGH_TRIGGER means on == HIGH
        (void)Config::getInstance().hardwareRelaysPumpTriggerType.set(::Hardware::RelayTriggerType::HIGH_TRIGGER);
        hardware_ = std::make_unique<HardwareManager>(Config::getInstance());
    }

    bool pumpPinOn() const {
        return TestPins::level[PIN_PUMP];
    }

    std::unique_ptr<HardwareManager> hardware_;
};

TEST_F(HardwareWaterTankTest, SetWaterTankEmptyIsNoOpWhenStateUnchanged) {
    hardware_->setWaterTankEmpty(false);

    EXPECT_FALSE(hardware_->isWaterTankEmpty());
    EXPECT_FALSE(pumpPinOn());
}

TEST_F(HardwareWaterTankTest, EnablePumpRefusesWhenTankEmpty) {
    hardware_->setWaterTankEmpty(true);

    hardware_->enablePump();

    EXPECT_FALSE(pumpPinOn());
    EXPECT_TRUE(hardware_->isWaterTankEmpty());
}

TEST_F(HardwareWaterTankTest, SetPumpPressureRefusesWhenTankEmpty) {
    hardware_->setWaterTankEmpty(true);

    hardware_->setPumpPressure(9.0f);

    EXPECT_FALSE(pumpPinOn());
}

TEST_F(HardwareWaterTankTest, TransitionToEmptyStopsRunningPump) {
    hardware_->enablePump();
    ASSERT_TRUE(pumpPinOn());

    hardware_->setWaterTankEmpty(true);

    EXPECT_FALSE(pumpPinOn());
    EXPECT_TRUE(hardware_->isWaterTankEmpty());
}

TEST_F(HardwareWaterTankTest, PumpAllowedAgainAfterRefill) {
    hardware_->setWaterTankEmpty(true);
    hardware_->setWaterTankEmpty(false);

    hardware_->enablePump();

    EXPECT_TRUE(pumpPinOn());
    EXPECT_FALSE(hardware_->isWaterTankEmpty());
}
