/**
 * @file DisplayBrewTimerState.h
 * @brief Single brew-timer display state machine (state stored in UICoordinator)
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/coordinators/UICoordinator.h"
#include "clevercoffee/handlers/BrewHandler.h"

#include <Arduino.h>

inline bool shouldDisplayBrewTimer(CleverCoffee::SystemContext& systemContext) {
    using CleverCoffee::UICoordinator;
    auto& ui = systemContext.uiCoordinator();

    switch (ui.getBrewTimerDisplayState()) {
        case UICoordinator::BrewTimerDisplayState::Idle:
            if (systemContext.brewHandler().isBrewActive()) {
                ui.setBrewTimerDisplayState(UICoordinator::BrewTimerDisplayState::Running);
            }
            break;

        case UICoordinator::BrewTimerDisplayState::Running:
            if (!systemContext.brewHandler().isBrewActive()) {
                ui.setBrewTimerDisplayState(UICoordinator::BrewTimerDisplayState::PostBrew);
                ui.setBrewTimerEndTime(millis());
            }
            break;

        case UICoordinator::BrewTimerDisplayState::PostBrew:
            if (millis() - ui.getBrewTimerEndTime() >
                static_cast<uint32_t>(Config::getInstance().displayPostBrewTimerDuration.get() * 1000)) {
                ui.setBrewTimerDisplayState(UICoordinator::BrewTimerDisplayState::Idle);
            }
            break;
    }

    return ui.getBrewTimerDisplayState() != UICoordinator::BrewTimerDisplayState::Idle;
}
