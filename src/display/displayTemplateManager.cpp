/**
 * @file displayTemplateManager.cpp
 * @brief Implementation of display template management system
 */

#include "displayTemplateManager.h"
#include "../Config.h"
#include "../defaults.h"

#if __cplusplus >= 202002L
// Modern C++20 implementation is header-only, no implementation needed
#else
// Traditional implementation for C++17 compatibility

// Static member definitions
System::DisplayTemplate DisplayTemplateManager::currentTemplate = System::DisplayTemplate::STANDARD;
void (*DisplayTemplateManager::currentPrintScreen)() = nullptr;
#endif

void DisplayTemplateManager::printScreen() {
#if __cplusplus >= 202002L
    // Use modern C++23 template system
    ModernDisplayTemplateManager::printScreen();
#else
    // Use traditional function pointer system
    if (currentPrintScreen) {
        currentPrintScreen();
    }
#endif
}