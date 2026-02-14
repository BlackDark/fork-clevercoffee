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

// ESP32 Watchdog includes
#if defined(ESP32)
#include "esp_task_wdt.h"
#endif

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
                double        backoffMultiplier = 2.0,
                unsigned int  maxAttempts       = 0)
        : initialDelayMs_(initialDelayMs), maxDelayMs_(maxDelayMs), backoffMultiplier_(backoffMultiplier),
          maxAttempts_(maxAttempts), currentAttempt_(0), lastAttemptTime_(0) {}

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
        currentAttempt_  = 0;
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
    unsigned long initialDelayMs_;    ///< Initial delay in milliseconds
    unsigned long maxDelayMs_;        ///< Maximum delay in milliseconds
    double        backoffMultiplier_; ///< Exponential backoff multiplier
    unsigned int  maxAttempts_;       ///< Maximum number of attempts (0 = unlimited)
    unsigned int  currentAttempt_;    ///< Current attempt number
    unsigned long lastAttemptTime_;   ///< Timestamp of last attempt
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
    CircuitBreaker(unsigned int failureThreshold, unsigned long openTimeoutMs, unsigned long halfOpenTimeoutMs = 30000)
        : failureThreshold_(failureThreshold), openTimeoutMs_(openTimeoutMs), halfOpenTimeoutMs_(halfOpenTimeoutMs),
          state_(CircuitState::CLOSED), failureCount_(0), successCount_(0), stateChangeTime_(0), halfOpenAttempts_(0) {}

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
                    state_           = CircuitState::CLOSED;
                    failureCount_    = 0;
                    successCount_    = 0;
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
                    state_           = CircuitState::OPEN;
                    stateChangeTime_ = currentTimeMs;
                    failureCount_    = 0; // Reset for next cycle
                }
                break;

            case CircuitState::HALF_OPEN:
                // Failure in half-open state, go back to open
                state_            = CircuitState::OPEN;
                stateChangeTime_  = currentTimeMs;
                halfOpenAttempts_ = 0;
                successCount_     = 0;
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
        state_            = CircuitState::CLOSED;
        failureCount_     = 0;
        successCount_     = 0;
        halfOpenAttempts_ = 0;
        stateChangeTime_  = currentTimeMs;
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
                    state_            = CircuitState::HALF_OPEN;
                    stateChangeTime_  = currentTimeMs;
                    successCount_     = 0;
                    halfOpenAttempts_ = 0;
                }
                break;
            }

            case CircuitState::HALF_OPEN: {
                // Check if we should close (enough time passed with no attempts)
                unsigned long elapsed = currentTimeMs - stateChangeTime_;
                if (elapsed >= halfOpenTimeoutMs_ && halfOpenAttempts_ == 0) {
                    // No attempts in half-open period, assume recovery
                    state_        = CircuitState::CLOSED;
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

    unsigned int  failureThreshold_;  ///< Failures needed to open circuit
    unsigned long openTimeoutMs_;     ///< Time to stay open before half-open
    unsigned long halfOpenTimeoutMs_; ///< Time in half-open before closing
    CircuitState  state_;             ///< Current circuit state
    unsigned int  failureCount_;      ///< Current failure count
    unsigned int  successCount_;      ///< Success count in half-open state
    unsigned long stateChangeTime_;   ///< Time when state last changed
    unsigned int  halfOpenAttempts_;  ///< Attempts in half-open state
};

} // namespace CleverCoffee::Utils

/**
 * @class Watchdog
 * @brief Hardware watchdog timer management for ESP32
 *
 * Provides safe watchdog timer management to detect and recover from
 * system hangs. The watchdog will reset the system if not fed within
 * the specified timeout period.
 *
 * SAFETY: The watchdog provides a last-resort recovery mechanism.
 * If the main loop hangs (e.g., due to deadlock, infinite loop, or
 * hardware fault), the watchdog will reset the system, turning off
 * all heaters and pumps for safety.
 *
 * Example usage:
 * @code
 * Watchdog wdt(5000); // 5 second timeout
 *
 * void setup() {
 *     wdt.begin();
 * }
 *
 * void loop() {
 *     // ... do work ...
 *     wdt.feed(); // Must be called within 5 seconds
 * }
 * @endcode
 */
class Watchdog {
  public:
    /**
     * @brief Construct watchdog with specified timeout
     * @param timeoutMs Timeout in milliseconds (default 5000ms = 5 seconds)
     *
     * Recommended timeout values:
     * - 3000ms: Fast detection, may trigger on slow network operations
     * - 5000ms: Balanced (default)
     * - 10000ms: Tolerant, for systems with slow operations
     */
    explicit Watchdog(unsigned long timeoutMs = 5000) : timeoutMs_(timeoutMs), enabled_(false), lastFeedTime_(0) {}

    /**
     * @brief Initialize and enable the hardware watchdog
     *
     * On ESP32, this configures the Task Watchdog Timer (TWDT).
     * The watchdog will reset the system if not fed within timeout.
     */
    void begin() {
#if defined(ESP32)
        // ESP32 Task Watchdog Timer configuration
        // Using the simpler API that's compatible with older ESP32 frameworks
        // timeout is in seconds, so convert from milliseconds
        esp_err_t ret = esp_task_wdt_init(timeoutMs_ / 1000, true);
        if (ret == ESP_OK) {
            enabled_ = true;
            LOGF(INFO, "Watchdog initialized with %lu ms timeout", timeoutMs_);
        } else if (ret == ESP_ERR_INVALID_STATE) {
            // Already initialized - just subscribe our task
            enabled_ = true;
            LOG(INFO, "Watchdog already initialized, subscribing task");
        } else {
            LOGF(ERROR, "Watchdog initialization failed: %d", ret);
            return;
        }

        // Subscribe current task to watchdog
        ret = esp_task_wdt_add(nullptr);
        if (ret != ESP_OK && ret != ESP_ERR_INVALID_STATE) {
            LOGF(WARNING, "Failed to subscribe task to watchdog: %d", ret);
        }

        lastFeedTime_ = millis();
#else
        // Non-ESP32 platforms - watchdog not supported
        LOG(WARNING, "Watchdog not supported on this platform");
#endif
    }

    /**
     * @brief Feed the watchdog (reset the timer)
     *
     * This must be called regularly within the timeout period.
     * If not called within timeout, the system will reset.
     *
     * @return true if feed was successful
     */
    bool feed() {
        if (!enabled_) {
            return false;
        }

#if defined(ESP32)
        esp_err_t ret = esp_task_wdt_reset();
        if (ret == ESP_OK) {
            lastFeedTime_ = millis();
            return true;
        } else {
            // Log only occasionally to avoid spamming
            static unsigned long lastErrorLog = 0;
            if (millis() - lastErrorLog > 10000) {
                LOGF(WARNING, "Watchdog feed failed: %d", ret);
                lastErrorLog = millis();
            }
            return false;
        }
#else
        lastFeedTime_ = millis();
        return true;
#endif
    }

    /**
     * @brief Check if watchdog is enabled
     * @return true if watchdog is active
     */
    bool isEnabled() const {
        return enabled_;
    }

    /**
     * @brief Get time since last successful feed
     * @return Milliseconds since last feed
     */
    unsigned long timeSinceLastFeed() const {
        return millis() - lastFeedTime_;
    }

    /**
     * @brief Get configured timeout
     * @return Timeout in milliseconds
     */
    unsigned long getTimeout() const {
        return timeoutMs_;
    }

    /**
     * @brief Check if watchdog is close to timing out
     * @param thresholdMs Warning threshold (default 80% of timeout)
     * @return true if time since last feed exceeds threshold
     */
    bool isNearTimeout(unsigned long thresholdMs = 0) const {
        if (!enabled_) {
            return false;
        }
        if (thresholdMs == 0) {
            thresholdMs = timeoutMs_ * 80 / 100; // 80% of timeout
        }
        return timeSinceLastFeed() > thresholdMs;
    }

  private:
    unsigned long timeoutMs_;    ///< Configured timeout in milliseconds
    bool          enabled_;      ///< Watchdog is active
    unsigned long lastFeedTime_; ///< Time of last successful feed
};
