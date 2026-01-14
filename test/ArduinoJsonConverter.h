/**
 * @file ArduinoJsonConverter.h
 * @brief ArduinoJson converter for String class in native tests
 */

#pragma once

#include "Arduino.h"
#include <ArduinoJson.h>

// ArduinoJson compatibility - allow String to be used directly
namespace ArduinoJson {
    template<>
    struct Converter<String> {
        static void toJson(const String& src, JsonVariant dst) {
            dst.set(src.c_str());
        }
        
        static String fromJson(JsonVariantConst src) {
            return String(src.as<const char*>());
        }
        
        static bool checkJson(JsonVariantConst src) {
            return src.is<const char*>();
        }
    };
}
