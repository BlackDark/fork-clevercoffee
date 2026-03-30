/**
 * @file AsyncTestHelpers.h
 * @brief Helpers for testing async patterns
 * 
 * Provides utilities for testing asynchronous operations like sensor reads
 */

#pragma once

#include <chrono>
#include <thread>
#include <functional>

namespace CleverCoffee {
namespace TestUtils {

/**
 * @brief Wait for async operation to complete
 * @param checkFunction Function that returns true when operation is complete
 * @param timeoutMs Maximum time to wait in milliseconds
 * @return true if operation completed, false if timeout
 */
template<typename CheckFunc>
bool waitForAsyncOperation(CheckFunc checkFunction, unsigned long timeoutMs = 1000) {
    auto start = std::chrono::steady_clock::now();
    auto timeout = std::chrono::milliseconds(timeoutMs);
    
    while (std::chrono::steady_clock::now() - start < timeout) {
        if (checkFunction()) {
            return true;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
    }
    return false;
}

/**
 * @brief Simulate async sensor read pattern
 * @param sensor Mock sensor to use
 * @param coordinator SensorCoordinator to update
 * @param expectedValue Expected sensor value
 */
template<typename SensorType, typename CoordinatorType>
void simulateAsyncSensorRead(SensorType* sensor, CoordinatorType& coordinator, double expectedValue) {
    sensor->setValue(expectedValue);
    sensor->startRead();
    coordinator.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coordinator.update();
}

} // namespace TestUtils
} // namespace CleverCoffee
