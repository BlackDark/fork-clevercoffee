/**
 * @file Timer.h
 *
 * @brief A helper class to call a method in regular intervals
 */

#pragma once

#include "Arduino.h"
#include "ModernTimer.h"

#include <functional>
#include <chrono>

class Timer {
    public:
        /** No default constructor */
        Timer() = delete;

        /**
         * @brief COnstructor taking a function pointer and the desired interval
         *
         * @param func Function pointer for the callback to be executed upon timer expiry
         * @param interval long Desired interval between calls in milliseconds
         * @param start_paused Boolean to select if the timer should start paused or run right away
         */
        Timer(std::function<void()> func, unsigned long interval, bool start_paused = false);

        /**
         * @brief Call operator for timer invocation
         * @details This will check if the timer expired and invoke the callback
         */
        void operator()();

        /**
         * @brief Reset the timer
         * @details This resets the timer such that the callback will be executed directly upon its next invocation
         */
        void reset() noexcept;

        /**
         * @brief Temporarily pause the timer
         */
        void pause() noexcept;

        /**
         * @brief (Re-) start the timer
         */
        void resume() noexcept;
        
        /**
         * @brief Get time remaining until next execution (legacy compatibility)
         * @return milliseconds until next execution
         */
        unsigned long getTimeRemaining() const noexcept;
        
        /**
         * @brief Check if timer is currently running
         * @return true if running, false if paused
         */
        bool isRunning() const noexcept { return running_; }
        
        /**
         * @brief Get the configured interval
         * @return Timer interval in milliseconds
         */
        unsigned long getInterval() const noexcept { return interval_; }

    private:
        // Callback to be executed when timer runs out:
        std::function<void()> callback_;

        // Interval of timer execution
        unsigned long interval_;

        // Next time the timer will run out
        unsigned long next_;

        // Bool to pause the timer
        bool running_;
};
