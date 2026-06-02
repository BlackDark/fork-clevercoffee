/**
 * @file DisplayTemplateManager.h
 * @brief Single entry point for display template dispatch
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/display/templates/MinimalTemplate.h"
#include "clevercoffee/display/templates/ModernTemplate.h"
#include "clevercoffee/display/templates/ScaleTemplate.h"
#include "clevercoffee/display/templates/StandardTemplate.h"
#include "clevercoffee/display/templates/TemperatureOnlyTemplate.h"
#include "clevercoffee/display/templates/UprightTemplate.h"

class DisplayTemplateManager {
  public:
    static void printScreen() {
        const System::DisplayTemplate templateId = Config::getInstance().displayTemplate.get();

        switch (templateId) {
            case System::DisplayTemplate::STANDARD:
                standardTemplate_.printScreen();
                break;
            case System::DisplayTemplate::UPRIGHT:
                uprightTemplate_.printScreen();
                break;
            case System::DisplayTemplate::MINIMAL:
                minimalTemplate_.printScreen();
                break;
            case System::DisplayTemplate::TEMPERATURE_ONLY:
                temperatureOnlyTemplate_.printScreen();
                break;
            case System::DisplayTemplate::SCALE:
                scaleTemplate_.printScreen();
                break;
            case System::DisplayTemplate::MODERN:
                modernTemplate_.printScreen();
                break;
            default:
                standardTemplate_.printScreen();
                break;
        }
    }

    static void setSystemContext(CleverCoffee::SystemContext* context) noexcept {
        standardTemplate_.setSystemContext(context);
        uprightTemplate_.setSystemContext(context);
        minimalTemplate_.setSystemContext(context);
        temperatureOnlyTemplate_.setSystemContext(context);
        scaleTemplate_.setSystemContext(context);
        modernTemplate_.setSystemContext(context);
    }

  private:
    static inline StandardTemplate        standardTemplate_;
    static inline UprightTemplate         uprightTemplate_;
    static inline MinimalTemplate         minimalTemplate_;
    static inline TemperatureOnlyTemplate temperatureOnlyTemplate_;
    static inline ScaleTemplate           scaleTemplate_;
    static inline ModernTemplate          modernTemplate_;
};
