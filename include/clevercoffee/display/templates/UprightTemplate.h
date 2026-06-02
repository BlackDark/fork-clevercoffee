/**
 * @file UprightTemplate.h
 * @brief Upright vertical layout display template
 */

#pragma once

#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/display/DisplayTemplateBase.h"
#include "clevercoffee/display/displayHelpers.h"
#include "clevercoffee/display/languages.h"

class UprightTemplate : public DisplayTemplateBase<UprightTemplate> {
  public:
    using DisplayPolicy = CleverCoffee::Display::DefaultDisplayPolicy;

    void renderNormalDisplay() {
        systemContext_->hardwareContext().display()->clearBuffer();

        displayTemperatureInfo(1, 14);
        displayHeatBar();
        displayMainStatus();
        displayBrewInfo(1, 34);
        displaySensorInfo();
        displayStatusBar();
    }

  private:
    void displayHeatBar() {
        systemContext_->hardwareContext().display()->drawFrame(0, 124, 64, 4);
        systemContext_->hardwareContext().display()->drawLine(
            1, 125, systemContext_->processPidOutput() / 16.13 + 1, 125);
        systemContext_->hardwareContext().display()->drawLine(
            1, 126, systemContext_->processPidOutput() / 16.13 + 1, 126);
    }

    void displayMainStatus() {
        const bool scaleEnabled    = Config::getInstance().hardwareSensorsScaleEnabled.get();
        const bool pressureEnabled = Config::getInstance().hardwareSensorsPressureEnabled.get();

        int yPos = scaleEnabled && pressureEnabled ? 65 : (scaleEnabled || pressureEnabled ? 60 : 55);
        systemContext_->hardwareContext().display()->setCursor(1, yPos);
        systemContext_->hardwareContext().display()->setFont(u8g2_font_fub20_tf);

        if (isManualFlushState(systemContext_->machineStateContext()->getCurrentStateId())) {
            systemContext_->hardwareContext().display()->print("FLUSH");
        } else if (shouldDisplayBrewTimer(*systemContext_)) {
            systemContext_->hardwareContext().display()->print("BREW");
        } else if (Config::getInstance().maintenanceBackflushReminderEnabled.get() &&
                   systemContext_->maintenanceCoordinator().isReminderDue()) {
            systemContext_->hardwareContext().display()->print("CLEAN");
        } else if (CleverCoffee::Display::isNearSetpointForDisplay(systemContext_->processTemperature(),
                                                                   systemContext_->processSetpoint()) &&
                   CleverCoffee::Display::isBlinkPhaseOn(*systemContext_)) {
            systemContext_->hardwareContext().display()->print("OK");
        } else {
            systemContext_->hardwareContext().display()->print("WAIT");
        }
    }

    void displaySensorInfo() {
        const bool scaleEnabled    = Config::getInstance().hardwareSensorsScaleEnabled.get();
        const bool pressureEnabled = Config::getInstance().hardwareSensorsPressureEnabled.get();

        if (scaleEnabled && shouldDisplayBrewTimer(*systemContext_)) {
            const bool isAutomatic  = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
            const bool brewByWeight = Config::getInstance().brewByWeightEnabled.get();

            if (isAutomatic && brewByWeight) {
                const auto targetWeight = Config::getInstance().brewByWeightTargetWeight.get();
                displayBrewWeight(*systemContext_,
                                  1,
                                  44,
                                  systemContext_->sensorCoordinator().getBrewWeight(),
                                  targetWeight,
                                  systemContext_->scaleFailure());
            } else {
                displayBrewWeight(*systemContext_,
                                  1,
                                  44,
                                  systemContext_->sensorCoordinator().getBrewWeight(),
                                  -1,
                                  systemContext_->scaleFailure());
            }
        } else if (scaleEnabled) {
            displayBrewWeight(*systemContext_,
                              1,
                              44,
                              systemContext_->sensorCoordinator().getWeight(),
                              -1,
                              systemContext_->scaleFailure());
        }

        if (pressureEnabled) {
            systemContext_->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            int yPos = scaleEnabled ? 54 : 44;
            systemContext_->hardwareContext().display()->setCursor(1, yPos);
            systemContext_->hardwareContext().display()->print(langstring_pressure_ur);
            systemContext_->hardwareContext().display()->print(systemContext_->sensorCoordinator().getPressure(), 1);
            systemContext_->hardwareContext().display()->print(" bar");
        }
    }

    void displayStatusBar() {
        systemContext_->hardwareContext().display()->drawLine(0,
                                                              CleverCoffee::Display::STATUS_BAR_Y_POSITION,
                                                              CleverCoffee::Display::OLED_WIDTH / 2,
                                                              CleverCoffee::Display::STATUS_BAR_Y_POSITION);
        if (!systemContext_->networkCoordinator().isOfflineMode()) {
            displayWiFiStatus(*systemContext_, 4, 2);
            displayMQTTStatus(*systemContext_, 21, 0);
        } else {
            systemContext_->hardwareContext().display()->setCursor(4, 1);
            systemContext_->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            systemContext_->hardwareContext().display()->print(langstring_offlinemode);
        }

        if (Config::getInstance().hardwareSensorsScaleEnabled.get() &&
            Config::getInstance().hardwareSensorsScaleType.get() == Hardware::ScaleType::BLUETOOTH) {
            displayBluetoothStatus(*systemContext_, 54, 1);
        }

        displayMaintenanceStatusBar(*systemContext_, 54, 0);
    }

  public:
    TemperatureCoords getTemperatureCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, baseX + 50, baseX, baseY + 10, baseX + 50};
    }

    PIDCoords getPIDCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, baseX, baseY + 27};
    }

    BrewCoords getBrewCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY};
    }

    const char* getCurrentTempLabel() const noexcept {
        return langstring_current_temp_ur;
    }
    const char* getSetTempLabel() const noexcept {
        return langstring_set_temp_ur;
    }
    const char* getBrewLabel() const noexcept {
        return langstring_brew_ur;
    }
    const char* getManualFlushLabel() const noexcept {
        return langstring_manual_flush_ur;
    }
    const char* getHotWaterLabel() const noexcept {
        return langstring_hot_water_ur;
    }
    const char* getPIDSeparator() const noexcept {
        return " ";
    }
};
