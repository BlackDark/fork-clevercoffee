/**
 * @file PumpTimer.h
 * @brief Timer utility for pump operations with safety timeout
 */

#pragma once

/**
 * @brief Timer utility for pump operations with safety timeout
 */
class PumpTimer {
private:
    unsigned long startTime_;
    unsigned long maxRunTime_;
    bool isRunning_;
    
public:
    PumpTimer(unsigned long maxTimeMs = 60000)
        : startTime_(0), maxRunTime_(maxTimeMs), isRunning_(false) {}
    
    void start() {
        startTime_ = millis();
        isRunning_ = true;
    }
    
    void stop() {
        isRunning_ = false;
        startTime_ = 0;
    }
    
    bool isExpired() const {
        if (!isRunning_ || startTime_ == 0) return false;
        return (millis() - startTime_) > maxRunTime_;
    }
    
    unsigned long getElapsedTime() const {
        if (!isRunning_ || startTime_ == 0) return 0;
        return millis() - startTime_;
    }
    
    double getElapsedSeconds() const {
        return getElapsedTime() / 1000.0;
    }
};