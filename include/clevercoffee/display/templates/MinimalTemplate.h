/**
 * @file MinimalTemplate.h
 * @brief Minimal display template — clean layout without thermometer graphic
 */

#pragma once

#include "clevercoffee/display/DisplayTemplateBase.h"
#include "clevercoffee/display/languages.h"

class MinimalTemplate : public DisplayTemplateBase<MinimalTemplate> {
  public:
    using DisplayPolicy = CleverCoffee::Display::DisplayPolicy<false>;

    void renderNormalDisplay() {
        systemContext_->hardwareContext().display()->clearBuffer();

        displayStatusbar(*systemContext_);
        displayTemperatureInfo(34, 16);
        displayBrewInfo(34, 36);
        displayProgressbar(*systemContext_, systemContext_->processPidOutput() / 10, 0, 58, 128);
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
