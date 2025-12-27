#include <gtest/gtest.h>
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include <thread>
#include <chrono>

using namespace CleverCoffee;

TEST(SensorCoordinatorTest, InitiallyNotRunning) {
    SensorCoordinator coord;
    EXPECT_FALSE(coord.isTemperatureUpdateRunning());
    EXPECT_FALSE(coord.isScaleUpdateRunning());
}

TEST(SensorCoordinatorTest, CanStartTemperatureUpdate) {
    SensorCoordinator coord;
    coord.startTemperatureUpdate();
    EXPECT_TRUE(coord.isTemperatureUpdateRunning());
}

TEST(SensorCoordinatorTest, CanStopTemperatureUpdate) {
    SensorCoordinator coord;
    coord.startTemperatureUpdate();
    coord.stopTemperatureUpdate();
    EXPECT_FALSE(coord.isTemperatureUpdateRunning());
}

TEST(SensorCoordinatorTest, IsThreadSafe) {
    SensorCoordinator coord;
    std::thread t1([&coord]() { coord.startTemperatureUpdate(); });
    std::thread t2([&coord]() { coord.stopTemperatureUpdate(); });
    t1.join();
    t2.join();
    // Should not crash or deadlock
    SUCCEED();
}
