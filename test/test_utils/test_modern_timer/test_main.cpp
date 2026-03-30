/**
 * @file test_main.cpp
 * @brief Comprehensive unit tests for ModernTimer
 */

#include <gtest/gtest.h>
#include "../../test_support.h"
#include "clevercoffee/utils/ModernTimer.h"
#include <chrono>
#include <thread>

using namespace std::chrono;

/**
 * @brief Test fixture for ModernTimer tests
 */
class ModernTimerTest : public ::testing::Test {
protected:
    void SetUp() override {
        callbackCount_ = 0;
    }

    void TearDown() override {
        callbackCount_ = 0;
    }

    int callbackCount_ = 0;
    
    std::function<void()> makeCallback() {
        return [this]() { callbackCount_++; };
    }
};

/**
 * @brief Test ModernTimer construction
 */
TEST_F(ModernTimerTest, Construction) {
    auto timer = ModernTimer<milliseconds>(makeCallback(), milliseconds(100));
    EXPECT_TRUE(timer.isRunning());
    EXPECT_EQ(milliseconds(100), timer.getInterval());
}

/**
 * @brief Test timer execution
 */
TEST_F(ModernTimerTest, TimerExecution) {
    auto timer = ModernTimer<milliseconds>(makeCallback(), milliseconds(100));
    
    // Initially callback not called
    EXPECT_EQ(0, callbackCount_);
    
    // Wait for timer to expire
    std::this_thread::sleep_for(milliseconds(150));
    
    // Call timer (should execute callback)
    timer();
    
    // Callback should have been called
    EXPECT_EQ(1, callbackCount_);
}

/**
 * @brief Test timer pause/resume
 */
TEST_F(ModernTimerTest, PauseResume) {
    auto timer = ModernTimer<milliseconds>(makeCallback(), milliseconds(100));
    
    // Pause timer
    timer.pause();
    EXPECT_FALSE(timer.isRunning());
    
    // Resume timer
    timer.resume();
    EXPECT_TRUE(timer.isRunning());
}

/**
 * @brief Test timer reset
 */
TEST_F(ModernTimerTest, TimerReset) {
    auto timer = ModernTimer<milliseconds>(makeCallback(), milliseconds(100));
    
    // Wait for timer to expire
    std::this_thread::sleep_for(milliseconds(150));
    
    // Reset timer
    timer.reset();
    
    // Timer should execute immediately on next call
    timer();
    EXPECT_EQ(1, callbackCount_);
}

/**
 * @brief Test interval change
 */
TEST_F(ModernTimerTest, IntervalChange) {
    auto timer = ModernTimer<milliseconds>(makeCallback(), milliseconds(100));
    
    // Change interval
    timer.setInterval(milliseconds(200));
    EXPECT_EQ(milliseconds(200), timer.getInterval());
}

/**
 * @brief Test time remaining
 */
TEST_F(ModernTimerTest, TimeRemaining) {
    auto timer = ModernTimer<milliseconds>(makeCallback(), milliseconds(100));
    
    // Initially should have time remaining
    auto remaining = timer.getTimeRemaining();
    EXPECT_GT(remaining.count(), 0);
    EXPECT_LE(remaining.count(), 100);
}

/**
 * @brief Test overdue detection
 */
TEST_F(ModernTimerTest, OverdueDetection) {
    auto timer = ModernTimer<milliseconds>(makeCallback(), milliseconds(100));
    
    // Initially not overdue
    EXPECT_FALSE(timer.isOverdue());
    
    // Wait for timer to expire
    std::this_thread::sleep_for(milliseconds(150));
    
    // Should be overdue
    EXPECT_TRUE(timer.isOverdue());
}

/**
 * @brief Test start paused
 */
TEST_F(ModernTimerTest, StartPaused) {
    auto timer = ModernTimer<milliseconds>(makeCallback(), milliseconds(100), true);
    
    // Should not be running
    EXPECT_FALSE(timer.isRunning());
    
    // Resume
    timer.resume();
    EXPECT_TRUE(timer.isRunning());
}

/**
 * @brief Test multiple executions
 */
TEST_F(ModernTimerTest, MultipleExecutions) {
    auto timer = ModernTimer<milliseconds>(makeCallback(), milliseconds(50));
    
    // Execute multiple times with delays
    for (int i = 0; i < 5; i++) {
        std::this_thread::sleep_for(milliseconds(60));
        timer();
    }
    
    // Should have executed multiple times
    EXPECT_GT(callbackCount_, 0);
}

/**
 * @brief Test different duration types
 */
TEST_F(ModernTimerTest, DifferentDurationTypes) {
    // Test with seconds
    auto secondTimer = ModernTimer<seconds>(makeCallback(), seconds(1));
    EXPECT_EQ(seconds(1), secondTimer.getInterval());
    
    // Test with microseconds
    auto microTimer = ModernTimer<microseconds>(makeCallback(), microseconds(1000));
    EXPECT_EQ(microseconds(1000), microTimer.getInterval());
}

/**
 * @brief Test factory functions
 */
TEST_F(ModernTimerTest, FactoryFunctions) {
    // Test make_timer
    auto timer1 = make_timer(makeCallback(), milliseconds(100));
    EXPECT_EQ(milliseconds(100), timer1.getInterval());
    
    // Test make_legacy_timer
    auto timer2 = make_legacy_timer(makeCallback(), 100);
    EXPECT_EQ(milliseconds(100), timer2.getInterval());
}

/**
 * @brief Test type aliases
 */
TEST_F(ModernTimerTest, TypeAliases) {
    // Test MillisecondTimer
    MillisecondTimer msTimer(makeCallback(), milliseconds(100));
    EXPECT_EQ(milliseconds(100), msTimer.getInterval());
    
    // Test SecondTimer
    SecondTimer secTimer(makeCallback(), seconds(1));
    EXPECT_EQ(seconds(1), secTimer.getInterval());
}
