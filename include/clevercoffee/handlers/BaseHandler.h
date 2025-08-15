/**
 * @file BaseHandler.h
 * @brief Base handler class to eliminate common code duplication
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/hardware/Switch.h"
#include "clevercoffee/hardware/Relay.h"
#include "clevercoffee/state/GlobalState.h"
#include <Logger.h>

// TODO do we need this globally?
// Forward declaration for isPowerSwitchOperationAllowed
bool isPowerSwitchOperationAllowed();

// TODO do we need this globally?
/**
 * @brief Simple implementation of power switch operation check
 */
inline bool isPowerSwitchOperationAllowed() {
    // Basic permission check - system should be initialized
    return g_state.machine.systemInitialized;
}

/**
 * @class BaseHandler
 * @brief Base class for all handlers providing common functionality
 *
 * This class eliminates duplication by providing:
 * - Common permission checks
 * - Hardware validation
 * - State management helpers
 * - Debug logging infrastructure
 */
class BaseHandler {
protected:
    const char* handlerName_;

public:
    explicit BaseHandler(const char* name) : handlerName_(name) {}
    virtual ~BaseHandler() = default;

    /**
     * @brief Main processing function - template method pattern
     */
    void process() {
        if (!isEnabled()) {
            return;
        }

        if (!hasPermission()) {
            return;
        }

        if (!isHardwareValid()) {
            logError("Hardware validation failed");
            return;
        }

        processImpl();
    }

protected:
    /**
     * @brief Check if this handler is enabled in configuration
     * Override in derived classes
     */
    virtual bool isEnabled() const = 0;

    /**
     * @brief Check if operations are permitted
     * Override for custom permission logic
     */
    virtual bool hasPermission() const {
        return isPowerSwitchOperationAllowed();
    }

    /**
     * @brief Validate hardware components
     * Override in derived classes
     */
    virtual bool isHardwareValid() const = 0;

    /**
     * @brief Actual handler implementation
     * Override in derived classes
     */
    virtual void processImpl() = 0;

    /**
     * @brief Helper to check switch state change
     */
    bool hasStateChanged(uint8_t newReading, uint8_t& currentState) {
        if (newReading != currentState) {
            currentState = newReading;
            return true;
        }
        return false;
    }

    /**
     * @brief Helper to toggle boolean state
     */
    void toggleState(bool& state) {
        state = !state;
        logDebug(state ? "State activated" : "State deactivated");
    }

    /**
     * @brief Helper to set relay state safely
     */
    void setRelayState(Relay* relay, bool on) {
        if (relay) {
            if (on) {
                relay->on();
            } else {
                relay->off();
            }
        }
    }

    /**
     * @brief Debug logging helper
     */
    void logDebug(const char* message) const {
        LOGF(DEBUG, "[%s] %s", handlerName_, message);
    }

    /**
     * @brief Error logging helper
     */
    void logError(const char* message) const {
        LOGF(ERROR, "[%s] %s", handlerName_, message);
    }

    /**
     * @brief Info logging helper
     */
    void logInfo(const char* message) const {
        LOGF(INFO, "[%s] %s", handlerName_, message);
    }
};

/**
 * @class SwitchBasedHandler
 * @brief Specialized base class for switch-based handlers
 */
class SwitchBasedHandler : public BaseHandler {
protected:
    Switch* switch_;

public:
    SwitchBasedHandler(const char* name, Switch* sw)
        : BaseHandler(name), switch_(sw) {}

protected:
    bool isHardwareValid() const override {
        return switch_ != nullptr;
    }

    /**
     * @brief Get current switch reading
     */
    uint8_t getSwitchReading() const {
        return switch_ ? switch_->isPressed() : LOW;
    }

    /**
     * @brief Process toggle switch behavior
     */
    bool processToggleSwitch(uint8_t reading, bool& targetState, bool& firstActivation) {
        bool changed = false;

        if (reading == HIGH) {
            if (!targetState) {
                targetState = true;
                changed = true;
                logDebug("Toggle switch activated");
            }
        }
        else if (reading == LOW && !firstActivation) {
            if (targetState) {
                targetState = false;
                changed = true;
                logDebug("Toggle switch deactivated");
            }
        }

        return changed;
    }

    /**
     * @brief Process momentary switch behavior
     */
    bool processMomentarySwitch(uint8_t reading, uint8_t& currentState, bool& targetState) {
        if (hasStateChanged(reading, currentState)) {
            if (currentState == HIGH) {
                toggleState(targetState);
                return true;
            }
        }
        return false;
    }
};
