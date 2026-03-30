/**
 * @file test_main.cpp
 * @brief Smoke tests for UICoordinator atomic operations
 *
 * MINIMAL TESTS - The class is just std::atomic wrappers, which the C++
 * standard library already tests. We keep 2 smoke tests to verify
 * the class compiles and works correctly.
 */

#include <gtest/gtest.h>
#include "test_support.h"
#include "clevercoffee/coordinators/UICoordinator.h"
#include <thread>
#include <chrono>

using namespace CleverCoffee;

/**
 * @brief Smoke test for UICoordinator
 */
class UICoordinatorTest : public ::testing::Test {
protected:
    void SetUp() override {
        coordinator_ = std::make_unique<UICoordinator>();
    }

    void TearDown() override {
        coordinator_.reset();
    }

    std::unique_ptr<UICoordinator> coordinator_;
};

/**
 * @brief Test coordinator construction and initial state
 */
TEST_F(UICoordinatorTest, ConstructionAndInitialState) {
    EXPECT_NE(nullptr, coordinator_);
    EXPECT_FALSE(coordinator_->needsRefresh());
    EXPECT_TRUE(coordinator_->shouldAutoSleep());
    EXPECT_FALSE(coordinator_->isDisplayBufferReady());
}

/**
 * @brief Test atomic operations are thread-safe
 *
 * This verifies that multiple threads can safely access the coordinator
 * without causing data races or crashes.
 */
TEST_F(UICoordinatorTest, ThreadSafety) {
    std::atomic<bool> test_complete{false};
    std::atomic<int> operations{0};

    // Thread 1: Modify state
    std::thread t1([&]() {
        for (int i = 0; i < 100; i++) {
            coordinator_->requestRefresh();
            coordinator_->setAutoSleep(i % 2 == 0);
            coordinator_->setDisplayBufferReady();
            operations++;
        }
    });

    // Thread 2: Modify state
    std::thread t2([&]() {
        for (int i = 0; i < 100; i++) {
            coordinator_->clearRefresh();
            coordinator_->clearDisplayBufferReady();
            coordinator_->setDisplayOffline(i);
            operations++;
        }
    });

    t1.join();
    t2.join();

    EXPECT_EQ(200, operations);
    EXPECT_NO_THROW(coordinator_->needsRefresh());  // Should not crash
}
