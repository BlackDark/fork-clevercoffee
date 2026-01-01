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
    brewHandler.setHardware(CleverCoffee::getGlobalSystemContext()->hardwareContext().brewSwitch(), CleverCoffee::getGlobalSystemContext()->hardwareContext().valveRelay());
    hotWaterHandler.setHardware(CleverCoffee::getGlobalSystemContext()->hardwareContext().hotWaterSwitch());
    powerHandler.setHardware(CleverCoffee::getGlobalSystemContext()->hardwareContext().powerSwitch());
    steamHandler.setHardware(CleverCoffee::getGlobalSystemContext()->hardwareContext().steamSwitch());
    
    // Register in SystemContext if provided
    if (systemContext) {
        // Set SystemContext on handlers for state machine access
        brewHandler.setSystemContext(systemContext);
        hotWaterHandler.setSystemContext(systemContext);
        powerHandler.setSystemContext(systemContext);
        steamHandler.setSystemContext(systemContext);
        
        // Register handlers with SystemContext
        systemContext->setBrewHandler(&brewHandler);
        systemContext->setHotWaterHandler(&hotWaterHandler);
        systemContext->setPowerHandler(&powerHandler);
        systemContext->setSteamHandler(&steamHandler);
    }
}
