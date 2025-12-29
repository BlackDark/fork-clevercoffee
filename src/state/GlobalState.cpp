/**
 * @file GlobalState.cpp
 * @brief Global state instance definition and handler initialization
 */

#include "clevercoffee/GlobalState.h"

#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/handlers/PowerHandler.h"
#include "clevercoffee/handlers/SteamHandler.h"

#include <PubSubClient.h>

// WIFI_PASSWORD definition
const char* WIFI_PASSWORD = WM_PASS;

// Handler instances
static BrewHandler     brewHandler;
static HotWaterHandler hotWaterHandler;
static PowerHandler    powerHandler;
static SteamHandler    steamHandler;

// Single global state instance
GlobalState g_state;

// Global system context reference (set during initialization)
CleverCoffee::SystemContext* CleverCoffee::g_systemContext = nullptr;

// Initialize handler references in global state
void initializeHandlers(CleverCoffee::SystemContext* systemContext) {
    // Initialize handler hardware (switches must be ready before calling this)
    brewHandler.setHardware(g_state.hardware.brewSwitch, g_state.hardware.valveRelay);
    hotWaterHandler.setHardware(g_state.hardware.hotWaterSwitch);
    powerHandler.setHardware(g_state.hardware.powerSwitch);
    steamHandler.setHardware(g_state.hardware.steamSwitch);
    
    // Store handler references in global state (backward compatibility)
    g_state.handlers.brewHandler     = &brewHandler;
    g_state.handlers.hotWaterHandler = &hotWaterHandler;
    g_state.handlers.powerHandler    = &powerHandler;
    g_state.handlers.steamHandler    = &steamHandler;
    
    // Also register in SystemContext if provided
    if (systemContext) {
        systemContext->setBrewHandler(&brewHandler);
        systemContext->setHotWaterHandler(&hotWaterHandler);
        systemContext->setPowerHandler(&powerHandler);
        systemContext->setSteamHandler(&steamHandler);
    }
}
