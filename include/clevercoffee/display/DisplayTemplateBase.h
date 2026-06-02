/**
 * @file DisplayTemplateBase.h
 * @brief CRTP display template base — shared rendering pipeline
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/display/DisplayBrewTimerState.h"
#include "clevercoffee/display/DisplayFullscreenModes.h"
#include "clevercoffee/display/DisplaySystemScreens.h"
#include "clevercoffee/display/DisplayTemplatePolicy.h"
#include "clevercoffee/display/DisplayWidgets.h"
#include "clevercoffee/display/displayHelpers.h"
#include "clevercoffee/state/MachineStateIds.h"

#include <PID_v1.h>

namespace CleverCoffee {
class SystemContext;
}

template <typename Derived>
class DisplayTemplateBase {
  public:
    void setSystemContext(CleverCoffee::SystemContext* context) noexcept {
        systemContext_ = context;
    }

    CleverCoffee::SystemContext* getSystemContext() const noexcept {
        return systemContext_;
    }

    void printScreen() {
        if (!systemContext_) {
            return;
        }

        if (handleFullscreenModes()) {
            return;
        }

        if (handleMachineStates()) {
            systemContext_->setDisplayBufferReady(false);
            return;
        }

        static_cast<Derived*>(this)->renderNormalDisplay();

        systemContext_->setDisplayBufferReady(true);
    }

    bool handleFullscreenModes() {
        if (!systemContext_) return false;

        using Policy = typename Derived::DisplayPolicy;

        if (Policy::sharedFullscreenBrewTimer() && displayFullscreenBrewTimer(*systemContext_)) return true;
        if (Policy::sharedFullscreenManualFlushTimer() && displayFullscreenManualFlushTimer(*systemContext_))
            return true;
        if (Policy::sharedFullscreenHotWaterTimer() && displayFullscreenHotWaterTimer(*systemContext_)) return true;

        if (displayOfflineMode(*systemContext_)) {
            systemContext_->setDisplayBufferReady(false);
            return true;
        }

        return false;
    }

    bool handleMachineStates() {
        if (!systemContext_) return false;

        auto* derived = static_cast<Derived*>(this);
        if (derived->tryDrawSystemScreen()) {
            systemContext_->setDisplayBufferReady(false);
            return true;
        }

        using Policy = typename Derived::DisplayPolicy;
        if (drawSystemScreen(*systemContext_, Policy::sharedHeatingLogoScreen())) {
            systemContext_->setDisplayBufferReady(false);
            return true;
        }

        return false;
    }

  protected:
    /** Override (protected in Derived) to fully replace shared system screens. */
    bool tryDrawSystemScreen() {
        return false;
    }

    CleverCoffee::SystemContext* systemContext_{nullptr};

    void displayTemperatureInfo(int baseX, int baseY) {
        if (!systemContext_) return;

        systemContext_->hardwareContext().display()->setFont(u8g2_font_profont11_tf);

        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getTemperatureCoords(baseX, baseY);

        systemContext_->hardwareContext().display()->setCursor(coords.currentTempX, coords.currentTempY);
        systemContext_->hardwareContext().display()->print(derived->getCurrentTempLabel());
        systemContext_->hardwareContext().display()->setCursor(coords.currentValueX, coords.currentTempY);
        systemContext_->hardwareContext().display()->print(systemContext_->processTemperature(), 1);
        systemContext_->hardwareContext().display()->setCursor(coords.currentValueX + 31, coords.currentTempY);
        systemContext_->hardwareContext().display()->print(static_cast<char>(176));
        systemContext_->hardwareContext().display()->print("C");

        systemContext_->hardwareContext().display()->setCursor(coords.setTempX, coords.setTempY);
        systemContext_->hardwareContext().display()->print(derived->getSetTempLabel());
        systemContext_->hardwareContext().display()->setCursor(coords.setValueX, coords.setTempY);
        systemContext_->hardwareContext().display()->print(systemContext_->processSetpoint(), 1);
        systemContext_->hardwareContext().display()->setCursor(coords.setValueX + 31, coords.setTempY);
        systemContext_->hardwareContext().display()->print(static_cast<char>(176));
        systemContext_->hardwareContext().display()->print("C");
    }

    void displayPIDInfo(int baseX, int baseY) {
        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getPIDCoords(baseX, baseY);

        systemContext_->hardwareContext().display()->setCursor(coords.pidX, coords.pidY);
        systemContext_->hardwareContext().display()->print(systemContext_->pidKp(), 0);
        systemContext_->hardwareContext().display()->print(derived->getPIDSeparator());

        if (systemContext_->pidKi() != 0) {
            systemContext_->hardwareContext().display()->print(systemContext_->pidKp() / systemContext_->pidKi(), 0);
        } else {
            systemContext_->hardwareContext().display()->print("0");
        }

        systemContext_->hardwareContext().display()->print(derived->getPIDSeparator());
        systemContext_->hardwareContext().display()->print(systemContext_->pidKd() / systemContext_->pidKp(), 0);

        systemContext_->hardwareContext().display()->setCursor(coords.outputX, coords.outputY);
        if (systemContext_->processPidOutput() < 99) {
            systemContext_->hardwareContext().display()->print(systemContext_->processPidOutput() / 10, 1);
        } else {
            systemContext_->hardwareContext().display()->print(systemContext_->processPidOutput() / 10, 0);
        }
        systemContext_->hardwareContext().display()->print("%");
    }

    void displayBrewInfo(int baseX, int baseY) {
        if (!Config::getInstance().hardwareSwitchesBrewEnabled.get()) return;

        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getBrewCoords(baseX, baseY);

        if (isManualFlushState(systemContext_->machineStateContext()->getCurrentStateId())) {
            displayBrewTime(*systemContext_,
                            coords.brewX,
                            coords.brewY,
                            derived->getManualFlushLabel(),
                            systemContext_->processCurrentBrewTime());
        } else if (shouldDisplayHotWaterTimer(*systemContext_)) {
            displayBrewTime(*systemContext_,
                            coords.brewX,
                            coords.brewY,
                            derived->getHotWaterLabel(),
                            systemContext_->currPumpOnTime());
        } else if (shouldDisplayBrewTimer(*systemContext_)) {
            const bool isAutomatic = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
            const bool brewByTime  = Config::getInstance().brewByTimeEnabled.get();

            if (isAutomatic && brewByTime) {
                displayBrewTime(*systemContext_,
                                coords.brewX,
                                coords.brewY,
                                derived->getBrewLabel(),
                                systemContext_->processCurrentBrewTime(),
                                systemContext_->processTotalTargetBrewTime());
            } else {
                displayBrewTime(*systemContext_,
                                coords.brewX,
                                coords.brewY,
                                derived->getBrewLabel(),
                                systemContext_->processCurrentBrewTime());
            }
        }
    }

    struct TemperatureCoords {
        int currentTempX, currentTempY;
        int currentValueX;
        int setTempX, setTempY;
        int setValueX;
    };

    struct PIDCoords {
        int pidX, pidY;
        int outputX, outputY;
    };

    struct BrewCoords {
        int brewX, brewY;
    };
};
