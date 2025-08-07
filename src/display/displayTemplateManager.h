#pragma once

#include "displayCommon.h"
#include "ModernDisplayTemplate.h"

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
            // Use modern template system
            ModernDisplayTemplateManager::setTemplate(templateId);
        }

        static void printScreen();

    // Modern implementation uses ModernDisplayTemplateManager directly
};
