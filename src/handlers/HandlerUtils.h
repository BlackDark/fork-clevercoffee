/**
 * @file HandlerUtils.h
 * @brief Common utilities for handlers to eliminate repetitive code
 */

#pragma once

#include "../Config.h"
#include "../hardware/Switch.h"
#include "../hardware/Relay.h"
#include "Logger.h"
#include <functional>

namespace HandlerUtils {

/**
 * @brief Common configuration checker for switch handlers
 */
struct SwitchConfig {
    std::function<bool()> enabledCheck;
    std::function<int()> typeCheck;
    Switch* switchPtr;
    
    SwitchConfig(std::function<bool()> enabled, std::function<int()> type, Switch* ptr)
        : enabledCheck(enabled), typeCheck(type), switchPtr(ptr) {}
    
    bool isValid() const {
        return enabledCheck() && switchPtr != nullptr;
    }
    
    bool isToggleType() const {
        return typeCheck() == Switch::TOGGLE;
    }
    
    bool isMomentaryType() const {
        return typeCheck() == Switch::MOMENTARY;
    }
};

/**
 * @brief Factory functions for common switch configurations
 */
inline SwitchConfig createSteamSwitchConfig() {
    return SwitchConfig(
        []() { return Config::getInstance().hardwareSwitchesSteamEnabled.get(); },
        []() { return static_cast<int>(Config::getInstance().hardwareSwitchesSteamType.get()); },
        g_state.hardware.steamSwitch
    );
}

inline SwitchConfig createBrewSwitchConfig() {
    return SwitchConfig(
        []() { return Config::getInstance().hardwareSwitchesBrewEnabled.get(); },
        []() { return static_cast<int>(Config::getInstance().hardwareSwitchesBrewType.get()); },
        g_state.hardware.brewSwitch
    );
}

inline SwitchConfig createHotWaterSwitchConfig() {
    return SwitchConfig(
        []() { return Config::getInstance().hardwareSwitchesHotWaterEnabled.get(); },
        []() { return static_cast<int>(Config::getInstance().hardwareSwitchesHotWaterType.get()); },
        g_state.hardware.hotWaterSwitch
    );
}

/**
 * @brief Generic switch processing function
 * Eliminates the repetitive if-else patterns across handlers
 */
template<typename StateHandler>
bool processGenericSwitch(const SwitchConfig& config, StateHandler stateHandler) {
    if (!config.isValid()) {
        return false;
    }
    
    if (!isPowerSwitchOperationAllowed()) {
        return false;
    }
    
    const uint8_t reading = config.switchPtr->isPressed();
    
    if (config.isToggleType()) {
        return stateHandler.handleToggle(reading);
    } else if (config.isMomentaryType()) {
        return stateHandler.handleMomentary(reading);
    }
    
    return false;
}

/**
 * @brief Safety checker for relay operations
 */
class RelaySafetyChecker {
private:
    std::function<bool()> safetyCondition_;
    const char* relayName_;
    
public:
    RelaySafetyChecker(std::function<bool()> condition, const char* name)
        : safetyCondition_(condition), relayName_(name) {}
    
    bool isSafeToOperate() const {
        bool safe = safetyCondition_();
        if (!safe) {
            LOGF(WARNING, "Relay operation blocked for %s - safety condition not met", relayName_);
        }
        return safe;
    }
};

/**
 * @brief Timer utility for pump operations
 */
class PumpTimer {
private:
    unsigned long startTime_;
    unsigned long maxRunTime_;
    bool isRunning_;
    
public:
    PumpTimer(unsigned long maxTimeMs = 60000) // Default 60 second max
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

/**
 * @brief Debounce utility for switches
 */
class SwitchDebouncer {
private:
    unsigned long lastChangeTime_;
    uint8_t lastReading_;
    uint8_t stableReading_;
    unsigned long debounceDelay_;
    
public:
    SwitchDebouncer(unsigned long debounceMs = 50) 
        : lastChangeTime_(0), lastReading_(LOW), stableReading_(LOW), debounceDelay_(debounceMs) {}
    
    uint8_t getStableReading(uint8_t currentReading) {
        if (currentReading != lastReading_) {
            lastChangeTime_ = millis();
            lastReading_ = currentReading;
        }
        
        if ((millis() - lastChangeTime_) > debounceDelay_) {
            stableReading_ = lastReading_;
        }
        
        return stableReading_;
    }
    
    bool hasStableChange(uint8_t currentReading, uint8_t& previousStable) {
        uint8_t newStable = getStableReading(currentReading);
        if (newStable != previousStable) {
            previousStable = newStable;
            return true;
        }
        return false;
    }
};

} // namespace HandlerUtils