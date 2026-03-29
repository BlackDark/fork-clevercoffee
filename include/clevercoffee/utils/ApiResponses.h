/**
 * @file ApiResponses.h
 * @brief API response builders - header for testing
 */

#pragma once

#include <Arduino.h>

namespace ApiResponses {

String boolResponse(const char* key, bool value, bool success = true);
String errorResponse(const char* message);
String successResponse(const char* message);

} // namespace ApiResponses