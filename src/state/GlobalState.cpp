/**
 * @file GlobalState.cpp
 * @brief Global state instance definition and handler initialization
 *
 * This file now serves as a thin initialization layer for handlers and global
 * state. The actual global state has been migrated to SystemContext.
 */

#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/Logger.h"

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

// Single global state instance - COMPLETELY ELIMINATED
// State is now managed entirely through SystemContext as private members

// Global system context reference (set during initialization)

CleverCoffee::SystemContext* CleverCoffee::g_systemContext = nullptr;

// Initialize handler references in global state
void initializeHandlers(CleverCoffee::SystemContext* systemContext) {
    // Validate that systemContext is provided and valid
    if (!systemContext) {
        LOG(ERROR, "initializeHandlers: systemContext is nullptr");
        return;
    }
    
    // Initialize handler hardware using the passed systemContext parameter
    // Use a reference to avoid repeated dereferences
    auto& hwContext = systemContext->hardwareContext();
    
    // Set hardware on all handlers
    brewHandler.setHardware(hwContext.brewSwitch(), hwContext.valveRelay());
    hotWaterHandler.setHardware(hwContext.hotWaterSwitch());
    powerHandler.setHardware(hwContext.powerSwitch());
    steamHandler.setHardware(hwContext.steamSwitch());
    
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
