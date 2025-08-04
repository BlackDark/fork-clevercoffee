/**
 * @file GlobalState.cpp
 * @brief Global state instance definition
 */

#include "GlobalState.h"
#include "../defaults.h"
#include <PubSubClient.h>

// WIFI_PASSWORD definition
const char* WIFI_PASSWORD = WM_PASS;

// Single global state instance
GlobalState g_state;
