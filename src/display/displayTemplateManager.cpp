/**
 * @file displayTemplateManager.cpp
 * @brief Implementation of display template management system
 */

#include "../Config.h"
#include "../defaults.h"

// Forward declaration to avoid circular includes
class DisplayTemplateManager {
public:
    static void printScreen();
private:
    static System::DisplayTemplate currentTemplate;
    static void (*currentPrintScreen)();
};

// Static member definitions
System::DisplayTemplate DisplayTemplateManager::currentTemplate = System::DisplayTemplate::STANDARD;
void (*DisplayTemplateManager::currentPrintScreen)() = nullptr;

void DisplayTemplateManager::printScreen() {
    if (currentPrintScreen) {
        currentPrintScreen();
    }
}