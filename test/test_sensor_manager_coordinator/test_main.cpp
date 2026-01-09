#include <gtest/gtest.h>
#include "../test_support.h"
#include "clevercoffee/sensors/SensorManager.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/hardware/tempsensors/TempSensor.h"

class MockTempSensor : public TempSensor {
public:
    MOCK_METHOD(float, readTemperature, (), (override, noexcept));
    MOCK_METHOD(bool, isValid, (), (const, noexcept, override));
    MOCK_METHOD(void, begin, (), (override));
};

TEST(SensorManagerTest, UsesSensorCoordinator) {
    SensorCoordinator coord;
    MockTempSensor sensor;
    EXPECT_CALL(sensor, begin()).Times(1);

    SensorManager manager;
    manager.initialize(&sensor, nullptr, &coord);

    EXPECT_FALSE(coord.isTemperatureUpdateRunning());

    // Simulate update start
    coord.startTemperatureUpdate();
    EXPECT_TRUE(coord.isTemperatureUpdateRunning());

    // Simulate update end
    coord.stopTemperatureUpdate();
    EXPECT_FALSE(coord.isTemperatureUpdateRunning());
}
