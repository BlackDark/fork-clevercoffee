#pragma once

#include "clevercoffee/display/displayCommon.h"
#include "clevercoffee/display/ModernDisplayTemplate.h"

class DisplayTemplateManager {
    public:
        static void initializeDisplay(const System::DisplayTemplate templateId) {
            // Use modern template system
            ModernDisplayTemplateManager::setTemplate(templateId);
        }

        static void printScreen();

    // Modern implementation uses ModernDisplayTemplateManager directly
};
