/**
 * @file TestConfig.h
 * @brief Test Config implementation compatible with EmergencyStopManager
 * 
 * Provides a Config-like class that can be used with EmergencyStopManager
 * in tests. Uses ParamDef members like the real Config class.
 * 
 * Note: This uses the real Config singleton but with stubbed Preferences
 * so NVS operations succeed in tests.
 */

#pragma once

#include "clevercoffee/Config.h"
#include <Arduino.h>

// TestConfig is just an alias to Config for now
// We use the real Config singleton with stubbed Preferences
using TestConfig = Config;
