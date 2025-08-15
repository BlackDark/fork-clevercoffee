/**
 * @file GlobalState.cpp
 * @brief Global state instance definition and handler initialization
 */

#include "clevercoffee/state/GlobalState.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/handlers/PowerHandler.h"
#include "clevercoffee/handlers/SteamHandler.h"
#include <PubSubClient.h>

// WIFI_PASSWORD definition
const char* WIFI_PASSWORD = WM_PASS;

// Handler instances
static BrewHandler brewHandler;
static HotWaterHandler hotWaterHandler;
static PowerHandler powerHandler;
static SteamHandler steamHandler;

// Single global state instance
GlobalState g_state;

// Initialize handler references in global state
void initializeHandlers() {
    g_state.handlers.brewHandler = &brewHandler;
    g_state.handlers.hotWaterHandler = &hotWaterHandler;
    g_state.handlers.powerHandler = &powerHandler;
    g_state.handlers.steamHandler = &steamHandler;
}
