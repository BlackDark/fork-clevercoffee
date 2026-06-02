/**
 * @file ScaleTemplate.h
 * @brief Scale display template — standard layout with weight and pressure
 */

#pragma once

#include "clevercoffee/display/DisplayTemplateBase.h"
#include "clevercoffee/display/languages.h"

class ScaleTemplate : public DisplayTemplateBase<ScaleTemplate> {
  public:
    using DisplayPolicy = CleverCoffee::Display::DefaultDisplayPolicy;

    void renderNormalDisplay() {
        systemContext_->hardwareContext().display()->clearBuffer();

        displayStatusbar(*systemContext_);
        displayTemperatureInfo(0, 16);

        if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
            displayBrewInfo(0, 26);
        }

        if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
            if (shouldDisplayBrewTimer(*systemContext_)) {
                const bool isAutomatic  = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
                const bool brewByWeight = Config::getInstance().brewByWeightEnabled.get();
                const auto targetWeight = Config::getInstance().brewByWeightTargetWeight.get();
                displayBrewWeight(*systemContext_,
                                  0,
                                  36,
                                  systemContext_->sensorCoordinator().getBrewWeight(),
                                  (isAutomatic && brewByWeight) ? targetWeight : -1,
                                  systemContext_->scaleFailure());
            } else {
                displayBrewWeight(*systemContext_,
                                  0,
                                  36,
                                  systemContext_->sensorCoordinator().getWeight(),
                                  -1,
                                  systemContext_->scaleFailure());
            }
        }

        if (Config::getInstance().hardwareSensorsPressureEnabled.get()) {
            systemContext_->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            systemContext_->hardwareContext().display()->setCursor(0, 46);
            systemContext_->hardwareContext().display()->print(langstring_pressure);
            systemContext_->hardwareContext().display()->print(systemContext_->sensorCoordinator().getPressure(), 1);
            systemContext_->hardwareContext().display()->print(" bar");
        }

        displayProgressbar(*systemContext_, systemContext_->processPidOutput() / 10, 0, 60, 128);
    }

    TemperatureCoords getTemperatureCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, 50, baseX, baseY + 10, 50};
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
