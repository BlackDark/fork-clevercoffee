/**
 * @file ModernTimer.h
 * @brief C++20/23 enhanced timer with std::chrono for type safety and precision
 * 
 * This is a modern replacement for the Timer class that uses std::chrono
 * for better type safety, precision, and portability.
 */

#pragma once

#include <chrono>
#include <functional>
#include <type_traits>

#if __has_include(<concepts>)
#include <concepts>

template<typename T>
concept DurationType = requires {
    std::is_same_v<T, std::chrono::nanoseconds> ||
    std::is_same_v<T, std::chrono::microseconds> ||
    std::is_same_v<T, std::chrono::milliseconds> ||
    std::is_same_v<T, std::chrono::seconds> ||
    std::is_same_v<T, std::chrono::minutes>;
};

#define HAS_DURATION_CONCEPT
#else
// Fallback for platforms without concepts support
#define HAS_DURATION_CONCEPT
#endif

/**
 * @class ModernTimer
 * @brief Type-safe timer using std::chrono for precise timing
 * @tparam Duration Duration type (defaults to std::chrono::milliseconds)
 */
template<typename Duration = std::chrono::milliseconds>
class ModernTimer {
public:
    using ClockType = std::chrono::steady_clock;
    using TimePoint = ClockType::time_point;
    using DurationType = Duration;
    
    /** No default constructor */
    ModernTimer() = delete;
    
    /**
     * @brief Constructor taking a callback function and interval
     * @param callback Function to be executed when timer expires
     * @param interval Desired interval between calls
     * @param start_paused Whether timer should start paused
     */
    constexpr ModernTimer(std::function<void()> callback, Duration interval, 
                         bool start_paused = false) noexcept
        : callback_(std::move(callback))
        , interval_(interval)
        , next_(ClockType::now() + interval)
        , running_(!start_paused)
        , last_execution_(ClockType::now()) {}
    
    /**
     * @brief Call operator for timer invocation
     * @details Checks if timer expired and invokes callback
     */
    void operator()() {
        if (!running_) return;
        
        auto now = ClockType::now();
        if (now >= next_) {
            // Execute callback
            callback_();
            
            // Update timing information
            last_execution_ = now;
            
            // Calculate next execution time with drift correction
            // This prevents timing drift by adding the interval to the 
            // scheduled time rather than the current time
            do {
                next_ += interval_;
            } while (next_ <= now); // Handle cases where we're very behind
        }
    }
    
    /**
     * @brief Reset the timer
     * @details Resets timer to execute immediately on next invocation
     */
    void reset() noexcept {
        next_ = ClockType::now();
    }
    
    /**
     * @brief Temporarily pause the timer
     */
    void pause() noexcept {
        running_ = false;
    }
    
    /**
     * @brief Resume the timer
     */
    void resume() noexcept {
        if (!running_) {
            // Adjust next execution time to account for pause duration
            auto now = ClockType::now();
            if (now > next_) {
                next_ = now + interval_;
            }
            running_ = true;
        }
    }
    
    /**
     * @brief Check if timer is currently running
     * @return true if running, false if paused
     */
    constexpr bool isRunning() const noexcept {
        return running_;
    }
    
    /**
     * @brief Get the configured interval
     * @return Timer interval
     */
    constexpr Duration getInterval() const noexcept {
        return interval_;
    }
    
    /**
     * @brief Change the timer interval
     * @param new_interval New interval duration
     */
    void setInterval(Duration new_interval) noexcept {
        interval_ = new_interval;
        if (running_) {
            next_ = ClockType::now() + interval_;
        }
    }
    
    /**
     * @brief Get time remaining until next execution
     * @return Duration until next execution (0 if overdue)
     */
    Duration getTimeRemaining() const noexcept {
        if (!running_) return Duration::zero();
        
        auto now = ClockType::now();
        if (now >= next_) return Duration::zero();
        
        return std::chrono::duration_cast<Duration>(next_ - now);
    }
    
    /**
     * @brief Get time since last execution
     * @return Duration since last callback execution
     */
    Duration getTimeSinceLastExecution() const noexcept {
        return std::chrono::duration_cast<Duration>(ClockType::now() - last_execution_);
    }
    
    /**
     * @brief Check if timer is overdue
     * @return true if should have executed already
     */
    bool isOverdue() const noexcept {
        return running_ && ClockType::now() >= next_;
    }

private:
    std::function<void()> callback_;
    Duration interval_;
    TimePoint next_;
    TimePoint last_execution_;
    bool running_;
};

// Convenient type aliases for common timer types
using MillisecondTimer = ModernTimer<std::chrono::milliseconds>;
using SecondTimer = ModernTimer<std::chrono::seconds>;
using MicrosecondTimer = ModernTimer<std::chrono::microseconds>;

// Factory functions for easy timer creation
template<typename Rep, typename Period>
constexpr auto make_timer(std::function<void()> callback, 
                         std::chrono::duration<Rep, Period> interval,
                         bool start_paused = false) noexcept {
    using DurationType = std::chrono::duration<Rep, Period>;
    return ModernTimer<DurationType>(std::move(callback), interval, start_paused);
}

// Legacy compatibility function that creates a millisecond timer
inline auto make_legacy_timer(std::function<void()> callback, 
                             unsigned long interval_ms,
                             bool start_paused = false) noexcept {
    return ModernTimer<std::chrono::milliseconds>(
        std::move(callback), 
        std::chrono::milliseconds(interval_ms), 
        start_paused
    );
}