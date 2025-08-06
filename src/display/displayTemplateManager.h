#pragma once

#include "displayCommon.h"

#if __cplusplus >= 202002L
#include "ModernDisplayTemplate.h"
#endif

namespace Templates {
    namespace Standard {
#include "displayTemplateStandard.h"
    }

    namespace Minimal {
#include "displayTemplateMinimal.h"
    }

    namespace TempOnly {
#include "displayTemplateTempOnly.h"
    }

    namespace Scale {
#include "displayTemplateScale.h"
    }

    namespace Upright {
#include "displayTemplateUpright.h"
    }
} // namespace Templates

class DisplayTemplateManager {
    public:
        // TODO probably the best location is here
        // enum Template {
        //     STANDARD = 0,
        //     MINIMAL = 1,
        //     TEMP_ONLY = 2,
        //     SCALE = 3,
        //     UPRIGHT = 4,
        // };

        static void initializeDisplay(const System::DisplayTemplate templateId) {
#if __cplusplus >= 202002L
            // Use modern C++23 template system with "deducing this"
            ModernDisplayTemplateManager::setTemplate(templateId);
#else
            // Fallback to traditional function pointer system
            currentTemplate = templateId;

            switch (templateId) {
                case System::DisplayTemplate::STANDARD:
                    currentPrintScreen = &Templates::Standard::printScreen;
                    break;
                case System::DisplayTemplate::MINIMAL:
                    currentPrintScreen = &Templates::Minimal::printScreen;
                    break;
                case System::DisplayTemplate::TEMPERATURE_ONLY:
                    currentPrintScreen = &Templates::TempOnly::printScreen;
                    break;
                case System::DisplayTemplate::SCALE:
                    currentPrintScreen = &Templates::Scale::printScreen;
                    break;
                case System::DisplayTemplate::UPRIGHT:
                    currentPrintScreen = &Templates::Upright::printScreen;
                    break;
                default:
                    currentPrintScreen = &Templates::Standard::printScreen;
                    break;
            }
#endif
        }

        static void printScreen();

    private:
        static System::DisplayTemplate currentTemplate;
        static void (*currentPrintScreen)();
};
