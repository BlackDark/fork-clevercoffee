#include <gtest/gtest.h>
#include "../test_support.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "../mocks/MockISensor.h"

using namespace CleverCoffee;
using MockSensor = MockISensor;

// Note: SensorManager was removed in favor of SensorCoordinator
// This test now verifies SensorCoordinator's automatic update behavior
TEST(SensorCoordinatorTest, AutomaticUpdates) {
    auto mockTemp = std::make_unique<MockSensor>(95.5);
    SensorCoordinator coord(mockTemp.get(), nullptr, nullptr);
    
    // SensorCoordinator updates are automatic and non-blocking
    // No need to manually start/stop updates - they happen automatically via update()
    
    // Initial state - no temperature read yet
    EXPECT_DOUBLE_EQ(0.0, coord.getTemperature());
    
    // Call update() to trigger automatic sensor reading
    coord.update();
    
    // Give async read time to complete
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    // Should now have the temperature value
    EXPECT_DOUBLE_EQ(95.5, coord.getTemperature());
}
