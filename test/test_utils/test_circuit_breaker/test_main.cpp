/**
 * @file test_main.cpp
 * @brief Comprehensive unit tests for CircuitBreaker
 */

#include <gtest/gtest.h>
// Include test_support.h FIRST to provide Arduino.h stubs
#include "../../test_support.h"
// Now include Resilience.h which needs Arduino.h
#include "clevercoffee/utils/Resilience.h"

using namespace CleverCoffee::Utils;

/**
 * @brief Test fixture for CircuitBreaker tests
 */
class CircuitBreakerTest : public ::testing::Test {
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
 * @brief Test CircuitBreaker construction
 */
TEST_F(CircuitBreakerTest, Construction) {
    CircuitBreaker breaker(5, 60000, 30000);
    EXPECT_TRUE(breaker.canAttempt(g_test_millis));
}

/**
 * @brief Test circuit opens after threshold failures
 */
TEST_F(CircuitBreakerTest, CircuitOpensAfterThreshold) {
    CircuitBreaker breaker(3, 60000, 30000);
    
    // Record failures up to threshold
    breaker.recordFailure(g_test_millis);
    breaker.recordFailure(g_test_millis);
    breaker.recordFailure(g_test_millis);
    
    // Circuit should be open
    EXPECT_FALSE(breaker.canAttempt(g_test_millis));
}

/**
 * @brief Test circuit closes after timeout
 */
TEST_F(CircuitBreakerTest, CircuitClosesAfterTimeout) {
    CircuitBreaker breaker(2, 1000, 500); // 1 second open timeout
    
    // Open circuit
    breaker.recordFailure(g_test_millis);
    breaker.recordFailure(g_test_millis);
    EXPECT_FALSE(breaker.canAttempt(g_test_millis));
    
    // Advance time past timeout
    advanceTime(2000);
    
    // Circuit should transition to half-open
    EXPECT_TRUE(breaker.canAttempt(g_test_millis));
}

/**
 * @brief Test half-open state
 */
TEST_F(CircuitBreakerTest, HalfOpenState) {
    CircuitBreaker breaker(2, 1000, 500);
    
    // Open circuit
    breaker.recordFailure(g_test_millis);
    breaker.recordFailure(g_test_millis);
    
    // Advance time to half-open
    advanceTime(2000);
    
    // Should allow attempt in half-open
    EXPECT_TRUE(breaker.canAttempt(g_test_millis));
    
    // Record success
    breaker.recordSuccess(g_test_millis);
    
    // Circuit should close
    EXPECT_TRUE(breaker.canAttempt(g_test_millis));
}

/**
 * @brief Test success resets failure count
 */
TEST_F(CircuitBreakerTest, SuccessResetsFailureCount) {
    CircuitBreaker breaker(3, 60000, 30000);
    
    // Record some failures
    breaker.recordFailure(g_test_millis);
    breaker.recordFailure(g_test_millis);
    
    // Record success
    breaker.recordSuccess(g_test_millis);
    
    // Failure count should be reset
    // Circuit should still be closed
    EXPECT_TRUE(breaker.canAttempt(g_test_millis));
}

/**
 * @brief Test failure in half-open reopens circuit
 */
TEST_F(CircuitBreakerTest, FailureInHalfOpenReopens) {
    CircuitBreaker breaker(2, 1000, 500);
    
    // Open circuit
    breaker.recordFailure(g_test_millis);
    breaker.recordFailure(g_test_millis);
    
    // Advance to half-open
    advanceTime(2000);
    
    // Attempt in half-open
    EXPECT_TRUE(breaker.canAttempt(g_test_millis));
    
    // Record failure in half-open
    breaker.recordFailure(g_test_millis);
    
    // Circuit should reopen
    EXPECT_FALSE(breaker.canAttempt(g_test_millis));
}

/**
 * @brief Test multiple state transitions
 */
TEST_F(CircuitBreakerTest, MultipleStateTransitions) {
    CircuitBreaker breaker(2, 1000, 500);
    
    // Open circuit
    breaker.recordFailure(g_test_millis);
    breaker.recordFailure(g_test_millis);
    EXPECT_FALSE(breaker.canAttempt(g_test_millis));
    
    // Advance to half-open
    advanceTime(2000);
    EXPECT_TRUE(breaker.canAttempt(g_test_millis));
    
    // Success closes circuit
    breaker.recordSuccess(g_test_millis);
    EXPECT_TRUE(breaker.canAttempt(g_test_millis));
    
    // Fail again to reopen
    breaker.recordFailure(g_test_millis);
    breaker.recordFailure(g_test_millis);
    EXPECT_FALSE(breaker.canAttempt(g_test_millis));
}
