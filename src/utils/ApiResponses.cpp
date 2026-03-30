/**
 * @file ApiResponses.cpp
 * @brief API response builder implementations
 */

#include "clevercoffee/utils/ApiResponses.h"

namespace ApiResponses {

String boolResponse(const char* key, bool value, bool success) {
    String result  = "{\"success\": ";
    result        += success ? "true" : "false";
    result        += ", \"";
    result        += key;
    result        += "\": ";
    result        += value ? "true" : "false";
    result        += "}";
    return result;
}

String errorResponse(const char* message) {
    String result  = "{\"error\": \"";
    result        += message;
    result        += "\"}";
    return result;
}

String successResponse(const char* message) {
    String result  = "{\"success\": true, \"message\": \"";
    result        += message;
    result        += "\"}";
    return result;
}

} // namespace ApiResponses