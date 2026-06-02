/**
 * @file TemperatureOnlyTemplate.h
 * @brief Temperature-only display template
 */

#pragma once

#include "clevercoffee/display/DisplayTemplateBase.h"
#include "clevercoffee/display/languages.h"

class TemperatureOnlyTemplate : public DisplayTemplateBase<TemperatureOnlyTemplate> {
  public:
    using DisplayPolicy = CleverCoffee::Display::DisplayPolicy<false>;

    void renderNormalDisplay() {
        systemContext_->hardwareContext().display()->clearBuffer();

        displayTemperature(*systemContext_, 0, 8);

        systemContext_->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        char targetBuf[20];
        snprintf(targetBuf,
                 sizeof(targetBuf),
                 "Target  %.1f %cC",
                 systemContext_->processSetpoint(),
                 static_cast<char>(176));
        const int targetW = systemContext_->hardwareContext().display()->getStrWidth(targetBuf);
        systemContext_->hardwareContext().display()->setCursor((128 - targetW) / 2, 50);
        systemContext_->hardwareContext().display()->print(targetBuf);

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
