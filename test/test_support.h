/**
 * @file test_support.h
 * @brief Support functions and stubs for unit tests
 *
 * Provides Arduino framework compatibility for PlatformIO tests
 *
 * Note: Arduino.h stub is provided in test/Arduino.h
 * This file provides additional test support functions
 *
 * Config reset between tests:
 *   - Prefer MockConfig (implements IConfig) for new tests — no singleton needed.
 *   - For legacy code using Config::getInstance(), include "ConfigTestHelper.h"
 *     and call resetConfigDefaults() in TearDown().
 */

#pragma once

// Include Arduino.h stub first (provides all Arduino definitions)
#include "Arduino.h"
// Include ArduinoJson converter for String class
#include "ArduinoJsonConverter.h"

