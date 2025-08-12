/**
 * @file SwitchHandler.h
 * @brief Generic switch handler to eliminate code duplication
 */

#pragma once

#include "../Config.h"
#include "../hardware/Switch.h"
#include "../state/GlobalState.h"
#include <functional>

/**
 * @class SwitchHandler
 * @brief Generic handler for digital switches that eliminates code duplication
 * 
 * This class abstracts common switch handling patterns:
 * - Config-based enable/disable checks
 * - Toggle vs Momentary switch behavior
 * - State change detection
 * - Permission checks
 */
class SwitchHandler {
private:
    Switch* switch_;
    bool& targetState_;
    uint8_t& currentSensorState_;
    bool& firstActivation_;
    std::function<bool()> permissionCheck_;
    std::function<bool()> enabledCheck_;
    std::function<int()> switchTypeCheck_;
    
public:
    /**
     * @brief Constructor for switch handler
     * @param switch_ptr Pointer to the switch hardware
     * @param targetState Reference to the target state boolean
     * @param currentSensorState Reference to current sensor state
     * @param firstActivation Reference to first activation flag (for web interface override)
     * @param permissionCheck Function to check if operation is allowed
     * @param enabledCheck Function to check if switch is enabled in config
     * @param switchTypeCheck Function to get switch type (TOGGLE/MOMENTARY)
     */
    SwitchHandler(Switch* switch_ptr, 
                  bool& targetState,
                  uint8_t& currentSensorState,
                  bool& firstActivation,
                  std::function<bool()> permissionCheck,
                  std::function<bool()> enabledCheck,
                  std::function<int()> switchTypeCheck)
        : switch_(switch_ptr)
        , targetState_(targetState)
        , currentSensorState_(currentSensorState)
        , firstActivation_(firstActivation)
        , permissionCheck_(permissionCheck)
        , enabledCheck_(enabledCheck)
        , switchTypeCheck_(switchTypeCheck) {}
    
    /**
     * @brief Process switch state changes
     * @return true if state changed, false otherwise
     */
    bool processSwitch() {
        // Early exit if disabled or null
        if (!enabledCheck_() || switch_ == nullptr) {
            return false;
        }
        
        // Permission check
        if (!permissionCheck_()) {
            return false;
        }
        
        const uint8_t switchReading = switch_->isPressed();
        const int switchType = switchTypeCheck_();
        
        if (switchType == Switch::TOGGLE) {
            return processToggleSwitch(switchReading);
        } else if (switchType == Switch::MOMENTARY) {
            return processMomentarySwitch(switchReading);
        }
        
        return false;
    }
    
private:
    /**
     * @brief Handle toggle switch behavior
     */
    bool processToggleSwitch(uint8_t switchReading) {
        bool changed = false;
        
        // Set target state when switch is HIGH
        if (switchReading == HIGH) {
            if (!targetState_) {
                targetState_ = true;
                changed = true;
            }
        }
        
        // If activated via web interface then firstActivation == true, prevent override
        if (switchReading == LOW && !firstActivation_) {
            if (targetState_) {
                targetState_ = false;
                changed = true;
            }
        }
        
        return changed;
    }
    
    /**
     * @brief Handle momentary switch behavior  
     */
    bool processMomentarySwitch(uint8_t switchReading) {
        if (switchReading != currentSensorState_) {
            currentSensorState_ = switchReading;
            
            // Only toggle on HIGH state
            if (currentSensorState_ == HIGH) {
                targetState_ = !targetState_;
                return true;
            }
        }
        
        return false;
    }
};

/**
 * @brief Factory function to create steam switch handler
 */
inline SwitchHandler createSteamSwitchHandler() {
    return SwitchHandler(
        g_state.hardware.steamSwitch,
        g_state.machine.steamON,
        g_state.sensors.currStateSteamSwitch,
        g_state.machine.steamFirstON,
        []() { return isPowerSwitchOperationAllowed(); },
        []() { return Config::getInstance().hardwareSwitchesSteamEnabled.get(); },
        []() { return static_cast<int>(Config::getInstance().hardwareSwitchesSteamType.get()); }
    );
}

/**
 * @brief Factory function to create brew switch handler
 */
inline SwitchHandler createBrewSwitchHandler() {
    return SwitchHandler(
        g_state.hardware.brewSwitch,
        g_state.machine.brewOn,
        g_state.sensors.currStateBrewSwitch,
        g_state.machine.brewFirstON,
        []() { return isPowerSwitchOperationAllowed(); },
        []() { return Config::getInstance().hardwareSwitchesBrewEnabled.get(); },
        []() { return static_cast<int>(Config::getInstance().hardwareSwitchesBrewType.get()); }
    );
}