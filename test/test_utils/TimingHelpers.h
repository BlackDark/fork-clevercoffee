/**
 * @file TimingHelpers.h
 * @brief Helpers for time-based testing
 * 
 * Provides utilities for testing timing-sensitive code
 */

#pragma once

#include <chrono>
#include <functional>

namespace CleverCoffee {
namespace TestUtils {

/**
 * @brief Measure execution time of a function
 * @param func Function to measure
 * @return Execution time in microseconds
 */
template<typename Func>
long measureExecutionTime(Func func) {
    auto start = std::chrono::high_resolution_clock::now();
    func();
    auto end = std::chrono::high_resolution_clock::now();
    return std::chrono::duration_cast<std::chrono::microseconds>(end - start).count();
}

/**
 * @brief Advance test time (for Arduino millis() simulation)
 * @param ms Milliseconds to advance
 */
void advanceTestTime(unsigned long ms);

/**
 * @brief Get current test time
 * @return Current test time in milliseconds
 */
unsigned long getTestTime();

} // namespace TestUtils
} // namespace CleverCoffee
