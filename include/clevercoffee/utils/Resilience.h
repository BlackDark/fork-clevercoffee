/**
 * @file Resilience.h
 * @brief Resilience patterns for fault tolerance: RetryPolicy and CircuitBreaker
 * 
 * This file consolidates resilience utilities for handling failures gracefully:
 * - RetryPolicy: Exponential backoff retry logic
 * - CircuitBreaker: Circuit breaker pattern for preventing cascading failures
 */

#pragma once

#include <Arduino.h>
#include <algorithm>
#include <atomic>
#include <cmath>

namespace CleverCoffee::Utils {

/**
 * @class RetryPolicy
 * @brief Implements retry logic with exponential backoff
 * 
 * This class provides configurable retry policies with exponential backoff
 * to prevent overwhelming systems with repeated failures.
 * 
 * Example usage:
 * @code
 * RetryPolicy retry(1000, 60000, 2.0); // 1s initial, 60s max, 2x multiplier
 * 
 * while (retry.shouldRetry()) {
 *     if (attemptOperation()) {
 *         retry.reset();
 *         break;
 *     }
 *     delay(retry.getNextDelay());
 *     retry.incrementAttempt();
 * }
 * @endcode
 */
class RetryPolicy {
public:
    /**
     * @brief Construct a retry policy
     * @param initialDelayMs Initial delay in milliseconds
     * @param maxDelayMs Maximum delay in milliseconds (caps exponential growth)
     * @param backoffMultiplier Multiplier for exponential backoff (default 2.0)
     * @param maxAttempts Maximum number of retry attempts (0 = unlimited)
     */
    RetryPolicy(unsigned long initialDelayMs,
                unsigned long maxDelayMs,
                double backoffMultiplier = 2.0,
                unsigned int maxAttempts = 0)
        : initialDelayMs_(initialDelayMs)
        , maxDelayMs_(maxDelayMs)
        , backoffMultiplier_(backoffMultiplier)
        , maxAttempts_(maxAttempts)
        , currentAttempt_(0)
        , lastAttemptTime_(0) {
    }

    /**
     * @brief Check if retry should be attempted
     * @return true if retries should continue, false if max attempts reached
     */
    bool shouldRetry() const {
        if (maxAttempts_ == 0) {
            return true; // Unlimited retries
        }
        return currentAttempt_ < maxAttempts_;
    }

    /**
     * @brief Get the delay for the next retry attempt
     * @return Delay in milliseconds (exponential backoff applied)
     */
    unsigned long getNextDelay() const {
        if (currentAttempt_ == 0) {
            return initialDelayMs_;
        }
        
        // Calculate exponential backoff: initialDelay * (multiplier ^ attempt)
        double delay = initialDelayMs_ * std::pow(backoffMultiplier_, currentAttempt_);
        
        // Cap at maximum delay
        return std::min(static_cast<unsigned long>(delay), maxDelayMs_);
    }

    /**
     * @brief Check if enough time has passed since last attempt
     * @param currentTimeMs Current time in milliseconds (from millis())
     * @return true if enough time has passed for next retry
     */
    bool canRetryNow(unsigned long currentTimeMs) const {
        if (currentAttempt_ == 0) {
            return true; // First attempt, no delay needed
        }
        
        unsigned long elapsed = currentTimeMs - lastAttemptTime_;
        return elapsed >= getNextDelay();
    }

    /**
     * @brief Increment retry attempt counter
     * @param currentTimeMs Current time in milliseconds (from millis())
     */
    void incrementAttempt(unsigned long currentTimeMs = millis()) {
        currentAttempt_++;
        lastAttemptTime_ = currentTimeMs;
    }

    /**
     * @brief Reset retry policy (call after successful operation)
     */
    void reset() {
        currentAttempt_ = 0;
        lastAttemptTime_ = 0;
    }

    /**
     * @brief Get current attempt number
     * @return Current attempt number (0 = first attempt)
     */
    unsigned int getCurrentAttempt() const {
        return currentAttempt_;
    }

    /**
     * @brief Check if max attempts reached
     * @return true if max attempts reached
     */
    bool isMaxAttemptsReached() const {
        if (maxAttempts_ == 0) {
            return false; // Unlimited retries
        }
        return currentAttempt_ >= maxAttempts_;
    }

private:
    unsigned long initialDelayMs_;     ///< Initial delay in milliseconds
    unsigned long maxDelayMs_;         ///< Maximum delay in milliseconds
    double backoffMultiplier_;         ///< Exponential backoff multiplier
    unsigned int maxAttempts_;         ///< Maximum number of attempts (0 = unlimited)
    unsigned int currentAttempt_;      ///< Current attempt number
    unsigned long lastAttemptTime_;    ///< Timestamp of last attempt
};

/**
 * @enum CircuitState
 * @brief States of the circuit breaker
 */
enum class CircuitState {
    CLOSED,   ///< Normal operation - requests pass through
    OPEN,     ///< Circuit is open - requests fail fast (service unavailable)
    HALF_OPEN ///< Testing if service recovered - allows limited requests
};

/**
 * @class CircuitBreaker
 * @brief Implements circuit breaker pattern to prevent cascading failures
 * 
 * The circuit breaker pattern prevents repeated failures from overwhelming
 * a system. It transitions between three states:
 * 
 * - CLOSED: Normal operation, requests pass through
 * - OPEN: Too many failures, requests fail fast
 * - HALF_OPEN: Testing recovery, allows limited requests
 * 
 * Example usage:
 * @code
 * CircuitBreaker breaker(5, 60000, 30000); // 5 failures, 60s timeout, 30s half-open
 * 
 * if (breaker.canAttempt()) {
 *     if (attemptOperation()) {
 *         breaker.recordSuccess();
 *     } else {
 *         breaker.recordFailure();
 *     }
 * } else {
 *     // Circuit is open, fail fast
 *     handleCircuitOpen();
 * }
 * @endcode
 */
class CircuitBreaker {
public:
    /**
     * @brief Construct a circuit breaker
     * @param failureThreshold Number of failures before opening circuit
     * @param openTimeoutMs How long to stay open before trying half-open (milliseconds)
     * @param halfOpenTimeoutMs How long to stay in half-open before closing (milliseconds)
     */
    CircuitBreaker(unsigned int failureThreshold,
                   unsigned long openTimeoutMs,
                   unsigned long halfOpenTimeoutMs = 30000)
        : failureThreshold_(failureThreshold)
        , openTimeoutMs_(openTimeoutMs)
        , halfOpenTimeoutMs_(halfOpenTimeoutMs)
        , state_(CircuitState::CLOSED)
        , failureCount_(0)
        , successCount_(0)
        , stateChangeTime_(0)
        , halfOpenAttempts_(0) {
    }

    /**
     * @brief Check if an operation can be attempted
     * @param currentTimeMs Current time in milliseconds (from millis())
     * @return true if operation can be attempted, false if circuit is open
     */
    bool canAttempt(unsigned long currentTimeMs = millis()) {
        updateState(currentTimeMs);
        
        switch (state_) {
            case CircuitState::CLOSED:
                return true;
            
            case CircuitState::OPEN:
                return false; // Fail fast
            
            case CircuitState::HALF_OPEN:
                // Allow limited attempts in half-open state
                // Only allow one attempt at a time
                return halfOpenAttempts_ == 0;
            
            default:
                return false;
        }
    }

    /**
     * @brief Record a successful operation
     * @param currentTimeMs Current time in milliseconds (from millis())
     */
    void recordSuccess(unsigned long currentTimeMs = millis()) {
        switch (state_) {
            case CircuitState::CLOSED:
                // Reset failure count on success
                failureCount_ = 0;
                break;
            
            case CircuitState::HALF_OPEN:
                successCount_++;
                halfOpenAttempts_ = 0;
                
                // If we get enough successes, close the circuit
                if (successCount_ >= 2) {
                    state_ = CircuitState::CLOSED;
                    failureCount_ = 0;
                    successCount_ = 0;
                    stateChangeTime_ = currentTimeMs;
                }
                break;
            
            case CircuitState::OPEN:
                // Should not happen, but handle gracefully
                break;
        }
    }

    /**
     * @brief Record a failed operation
     * @param currentTimeMs Current time in milliseconds (from millis())
     */
    void recordFailure(unsigned long currentTimeMs = millis()) {
        switch (state_) {
            case CircuitState::CLOSED:
                failureCount_++;
                if (failureCount_ >= failureThreshold_) {
                    // Open the circuit
                    state_ = CircuitState::OPEN;
                    stateChangeTime_ = currentTimeMs;
                    failureCount_ = 0; // Reset for next cycle
                }
                break;
            
            case CircuitState::HALF_OPEN:
                // Failure in half-open state, go back to open
                state_ = CircuitState::OPEN;
                stateChangeTime_ = currentTimeMs;
                halfOpenAttempts_ = 0;
                successCount_ = 0;
                break;
            
            case CircuitState::OPEN:
                // Already open, no change needed
                break;
        }
    }

    /**
     * @brief Get current circuit state
     * @return Current circuit state
     */
    CircuitState getState() const {
        return state_;
    }

    /**
     * @brief Check if circuit is open (failing fast)
     * @return true if circuit is open
     */
    bool isOpen() const {
        return state_ == CircuitState::OPEN;
    }

    /**
     * @brief Check if circuit is closed (normal operation)
     * @return true if circuit is closed
     */
    bool isClosed() const {
        return state_ == CircuitState::CLOSED;
    }

    /**
     * @brief Reset circuit breaker to closed state
     * @param currentTimeMs Current time in milliseconds (from millis())
     */
    void reset(unsigned long currentTimeMs = millis()) {
        state_ = CircuitState::CLOSED;
        failureCount_ = 0;
        successCount_ = 0;
        halfOpenAttempts_ = 0;
        stateChangeTime_ = currentTimeMs;
    }

    /**
     * @brief Get failure count
     * @return Current failure count
     */
    unsigned int getFailureCount() const {
        return failureCount_;
    }

private:
    /**
     * @brief Update circuit state based on timeouts
     * @param currentTimeMs Current time in milliseconds
     */
    void updateState(unsigned long currentTimeMs) {
        switch (state_) {
            case CircuitState::OPEN: {
                // Check if we should transition to half-open
                unsigned long elapsed = currentTimeMs - stateChangeTime_;
                if (elapsed >= openTimeoutMs_) {
                    state_ = CircuitState::HALF_OPEN;
                    stateChangeTime_ = currentTimeMs;
                    successCount_ = 0;
                    halfOpenAttempts_ = 0;
                }
                break;
            }
            
            case CircuitState::HALF_OPEN: {
                // Check if we should close (enough time passed with no attempts)
                unsigned long elapsed = currentTimeMs - stateChangeTime_;
                if (elapsed >= halfOpenTimeoutMs_ && halfOpenAttempts_ == 0) {
                    // No attempts in half-open period, assume recovery
                    state_ = CircuitState::CLOSED;
                    failureCount_ = 0;
                    successCount_ = 0;
                }
                break;
            }
            
            case CircuitState::CLOSED:
                // No state change needed
                break;
        }
    }

    unsigned int failureThreshold_;      ///< Failures needed to open circuit
    unsigned long openTimeoutMs_;        ///< Time to stay open before half-open
    unsigned long halfOpenTimeoutMs_;    ///< Time in half-open before closing
    CircuitState state_;                 ///< Current circuit state
    unsigned int failureCount_;          ///< Current failure count
    unsigned int successCount_;          ///< Success count in half-open state
    unsigned long stateChangeTime_;      ///< Time when state last changed
    unsigned int halfOpenAttempts_;      ///< Attempts in half-open state
};

} // namespace CleverCoffee::Utils
