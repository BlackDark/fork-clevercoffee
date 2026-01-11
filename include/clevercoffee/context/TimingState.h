/**
 * @file TimingState.h
 * @brief Timing state management for update throttling and window timing
 * 
 * This class encapsulates timing-related state that was previously in SystemContext.
 * It provides a clean interface for update throttling timestamps and window timing.
 * 
 * NOTE: ISR-related timing (isrCounter, isrReady) and hardware timers remain in SystemContext
 * due to ISR access requirements and special initialization needs.
 * 
 * Design Pattern: Single Responsibility Principle
 * - TimingState is responsible only for update throttling timing
 * - ISR timing and hardware timers remain in SystemContext for safety
 * - Improves testability and maintainability
 */

#pragma once

namespace CleverCoffee {

/**
 * @class TimingState
 * @brief Manages timing state for update throttling
 * 
 * This class encapsulates timing state related to:
 * - Update throttling (previousMillis* timestamps)
 * - Window timing (windowStartTime)
 * 
 * NOTE: ISR counter and ISR ready flag remain in SystemContext due to ISR access requirements.
 * Hardware timers (MillisecondTimer) remain in SystemContext due to initialization complexity.
 * 
 * Example usage:
 * @code
 * TimingState timingState;
 * timingState.setPreviousMillisTemp(millis());
 * unsigned long lastUpdate = timingState.previousMillisTemp();
 * @endcode
 */
class TimingState {
public:
    TimingState() = default;
    ~TimingState() = default;

    // Update throttling timestamps
    unsigned long previousMillisTemp() const noexcept { return previousMillisTemp_; }
    void setPreviousMillisTemp(unsigned long time) noexcept { previousMillisTemp_ = time; }

    unsigned long previousMillisMQTT() const noexcept { return previousMillisMQTT_; }
    void setPreviousMillisMQTT(unsigned long time) noexcept { previousMillisMQTT_ = time; }

    unsigned long previousMillisPressure() const noexcept { return previousMillisPressure_; }
    void setPreviousMillisPressure(unsigned long time) noexcept { previousMillisPressure_ = time; }

    // Window timing
    unsigned long windowStartTime() const noexcept { return windowStartTime_; }
    void setWindowStartTime(unsigned long time) noexcept { windowStartTime_ = time; }

private:
    // Update throttling timestamps (for rate limiting)
    unsigned long previousMillisTemp_ = 0;
    unsigned long previousMillisMQTT_ = 0;
    unsigned long previousMillisPressure_ = 0;

    // Window timing (for PID window control)
    unsigned long windowStartTime_ = 0;
};

} // namespace CleverCoffee
