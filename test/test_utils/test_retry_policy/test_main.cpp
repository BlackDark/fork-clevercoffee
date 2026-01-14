/**
 * @file test_main.cpp
 * @brief Comprehensive unit tests for RetryPolicy
 */

#include <gtest/gtest.h>
// Include test_support.h FIRST to provide Arduino.h stubs
#include "../../test_support.h"
// Now include Resilience.h which needs Arduino.h
#include "clevercoffee/utils/Resilience.h"
#include <cmath>

using namespace CleverCoffee::Utils;

/**
 * @brief Test fixture for RetryPolicy tests
 */
class RetryPolicyTest : public ::testing::Test {
protected:
    void SetUp() override {
        g_test_millis = 0;
    }

    void TearDown() override {
        g_test_millis = 0;
    }

    void advanceTime(unsigned long ms) {
        g_test_millis += ms;
    }
};

/**
 * @brief Test RetryPolicy construction
 */
TEST_F(RetryPolicyTest, Construction) {
    RetryPolicy policy(1000, 60000, 2.0, 5);
    EXPECT_TRUE(policy.shouldRetry());
    EXPECT_EQ(0, policy.getCurrentAttempt());
    EXPECT_FALSE(policy.isMaxAttemptsReached());
}

/**
 * @brief Test initial delay
 */
TEST_F(RetryPolicyTest, InitialDelay) {
    RetryPolicy policy(1000, 60000, 2.0, 5);
    
    // First attempt should use initial delay
    EXPECT_EQ(1000UL, policy.getNextDelay());
}

/**
 * @brief Test exponential backoff
 */
TEST_F(RetryPolicyTest, ExponentialBackoff) {
    RetryPolicy policy(1000, 60000, 2.0, 5);
    
    // First attempt
    EXPECT_EQ(1000UL, policy.getNextDelay());
    policy.incrementAttempt(g_test_millis);
    
    // Second attempt: 1000 * 2^1 = 2000
    EXPECT_EQ(2000UL, policy.getNextDelay());
    policy.incrementAttempt(g_test_millis);
    
    // Third attempt: 1000 * 2^2 = 4000
    EXPECT_EQ(4000UL, policy.getNextDelay());
    policy.incrementAttempt(g_test_millis);
    
    // Fourth attempt: 1000 * 2^3 = 8000
    EXPECT_EQ(8000UL, policy.getNextDelay());
}

/**
 * @brief Test max delay cap
 */
TEST_F(RetryPolicyTest, MaxDelayCap) {
    RetryPolicy policy(1000, 5000, 2.0, 10);
    
    // After several attempts, delay should cap at maxDelay
    for (int i = 0; i < 10; i++) {
        policy.incrementAttempt(g_test_millis);
    }
    
    // Delay should be capped at 5000ms
    EXPECT_LE(policy.getNextDelay(), 5000UL);
}

/**
 * @brief Test should retry
 */
TEST_F(RetryPolicyTest, ShouldRetry) {
    RetryPolicy policy(1000, 60000, 2.0, 3);
    
    // Should retry initially
    EXPECT_TRUE(policy.shouldRetry());
    
    // After max attempts, should not retry
    policy.incrementAttempt(g_test_millis);
    policy.incrementAttempt(g_test_millis);
    policy.incrementAttempt(g_test_millis);
    
    EXPECT_FALSE(policy.shouldRetry());
    EXPECT_TRUE(policy.isMaxAttemptsReached());
}

/**
 * @brief Test unlimited retries
 */
TEST_F(RetryPolicyTest, UnlimitedRetries) {
    RetryPolicy policy(1000, 60000, 2.0, 0); // 0 = unlimited
    
    // Should always retry
    for (int i = 0; i < 100; i++) {
        EXPECT_TRUE(policy.shouldRetry());
        policy.incrementAttempt(g_test_millis);
    }
    
    EXPECT_FALSE(policy.isMaxAttemptsReached());
}

/**
 * @brief Test can retry now
 */
TEST_F(RetryPolicyTest, CanRetryNow) {
    RetryPolicy policy(1000, 60000, 2.0, 5);
    
    // First attempt can retry immediately
    EXPECT_TRUE(policy.canRetryNow(g_test_millis));
    
    // Increment attempt
    policy.incrementAttempt(g_test_millis);
    
    // Cannot retry immediately (need to wait for delay)
    EXPECT_FALSE(policy.canRetryNow(g_test_millis));
    
    // Advance time past delay
    advanceTime(2000);
    EXPECT_TRUE(policy.canRetryNow(g_test_millis));
}

/**
 * @brief Test reset
 */
TEST_F(RetryPolicyTest, Reset) {
    RetryPolicy policy(1000, 60000, 2.0, 5);
    
    // Increment attempts
    policy.incrementAttempt(g_test_millis);
    policy.incrementAttempt(g_test_millis);
    EXPECT_EQ(2, policy.getCurrentAttempt());
    
    // Reset
    policy.reset();
    EXPECT_EQ(0, policy.getCurrentAttempt());
    EXPECT_EQ(1000UL, policy.getNextDelay());
}

/**
 * @brief Test different backoff multipliers
 */
TEST_F(RetryPolicyTest, DifferentBackoffMultipliers) {
    // Test with multiplier 1.5
    RetryPolicy policy1(1000, 60000, 1.5, 5);
    policy1.incrementAttempt(g_test_millis);
    EXPECT_EQ(1500UL, policy1.getNextDelay());
    
    // Test with multiplier 3.0
    RetryPolicy policy2(1000, 60000, 3.0, 5);
    policy2.incrementAttempt(g_test_millis);
    EXPECT_EQ(3000UL, policy2.getNextDelay());
}

/**
 * @brief Test current attempt tracking
 */
TEST_F(RetryPolicyTest, CurrentAttemptTracking) {
    RetryPolicy policy(1000, 60000, 2.0, 5);
    
    EXPECT_EQ(0, policy.getCurrentAttempt());
    
    policy.incrementAttempt(g_test_millis);
    EXPECT_EQ(1, policy.getCurrentAttempt());
    
    policy.incrementAttempt(g_test_millis);
    EXPECT_EQ(2, policy.getCurrentAttempt());
}
