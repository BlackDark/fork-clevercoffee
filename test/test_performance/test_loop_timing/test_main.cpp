/**
 * @file test_main.cpp
 * @brief Performance tests for loop timing
 */

#include <gtest/gtest.h>
#include "../../test_support.h"
#include <chrono>

class LoopTimingTest : public ::testing::Test {
protected:
    void SetUp() override {}
};

TEST_F(LoopTimingTest, DISABLED_MainLoopExecutionTime) {
    // Test main loop execution time
    // auto start = std::chrono::high_resolution_clock::now();
    // loopManager.update();
    // auto end = std::chrono::high_resolution_clock::now();
    // auto duration = std::chrono::duration_cast<std::chrono::microseconds>(end - start);
    // EXPECT_LT(duration.count(), 10000) << "Loop should complete in < 10ms";
}

// Note: main() is provided by test/main.cpp
