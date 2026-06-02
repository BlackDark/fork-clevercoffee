/**
 * @file StandardTemplate.h
 * @brief Standard horizontal layout display template
 */

#pragma once

#include "clevercoffee/display/DisplayTemplateBase.h"
#include "clevercoffee/display/displayHelpers.h"
#include "clevercoffee/display/languages.h"

class StandardTemplate : public DisplayTemplateBase<StandardTemplate> {
  public:
    using DisplayPolicy = CleverCoffee::Display::DefaultDisplayPolicy;

    void renderNormalDisplay() {
        systemContext_->hardwareContext().display()->clearBuffer();

        displayStatusbar(*systemContext_);
        displayTemperatureInfo(34, 16);

        displayThermometerOutline(*systemContext_, 4, 62);
        {
            const bool nearSetpoint = CleverCoffee::Display::isNearSetpointForDisplay(
                systemContext_->processTemperature(), systemContext_->processSetpoint());
            if (!nearSetpoint || CleverCoffee::Display::isBlinkPhaseOn(*systemContext_)) {
                drawTemperaturebar(*systemContext_, 8, 30);
            }
        }

        displayBrewInfo(34, 36);
        displayPIDInfo(38, 47);

        displayProgressbar(*systemContext_, systemContext_->processPidOutput() / 10, 30, 60, 98);
        displayMaintenanceFooter(*systemContext_);
    }

    TemperatureCoords getTemperatureCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, 84, baseX, baseY + 10, 84};
    }

    PIDCoords getPIDCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, 96, baseY};
    }

    BrewCoords getBrewCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY};
    }

    const char* getCurrentTempLabel() const noexcept {
        return langstring_current_temp;
    }
    const char* getSetTempLabel() const noexcept {
        return langstring_set_temp;
    }
    const char* getBrewLabel() const noexcept {
        return langstring_brew;
    }
    const char* getManualFlushLabel() const noexcept {
        return langstring_manual_flush;
    }
    const char* getHotWaterLabel() const noexcept {
        return langstring_hot_water;
    }
    const char* getPIDSeparator() const noexcept {
        return "|";
    }
};
