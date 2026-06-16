#pragma once

#include <ArduinoJson.h>

namespace CleverCoffee::ConfigJson {

/** True when any top-level key contains '.' (legacy flat export format). */
[[nodiscard]] bool usesFlatDotKeys(JsonObjectConst root) noexcept;

/** Read a value at a dotted path through nested objects (e.g. brew.setpoint). */
[[nodiscard]] JsonVariantConst getNested(JsonObjectConst root, const char* dotPath) noexcept;

/** Write a value at a dotted path, creating intermediate objects as needed. */
[[nodiscard]] bool setNested(JsonObject root, const char* dotPath, JsonVariant value) noexcept;

} // namespace CleverCoffee::ConfigJson
