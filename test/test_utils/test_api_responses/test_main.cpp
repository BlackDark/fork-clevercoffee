#include <gtest/gtest.h>
#include "../../test_support.h"
#include "clevercoffee/utils/ApiResponses.h"
#include "../../src/utils/ApiResponses.cpp"
#include <cstring>

TEST(ApiResponsesTest, BoolResponseTrue) {
    String result = ApiResponses::boolResponse("steamMode", true);
    EXPECT_TRUE(strstr(result.c_str(), "\"success\": true") != nullptr);
    EXPECT_TRUE(strstr(result.c_str(), "\"steamMode\": true") != nullptr);
}

TEST(ApiResponsesTest, BoolResponseFalse) {
    String result = ApiResponses::boolResponse("pidEnabled", false);
    EXPECT_TRUE(strstr(result.c_str(), "\"success\": true") != nullptr);
    EXPECT_TRUE(strstr(result.c_str(), "\"pidEnabled\": false") != nullptr);
}

TEST(ApiResponsesTest, BoolResponseFailure) {
    String result = ApiResponses::boolResponse("test", true, false);
    EXPECT_TRUE(strstr(result.c_str(), "\"success\": false") != nullptr);
}

TEST(ApiResponsesTest, ErrorResponse) {
    String result = ApiResponses::errorResponse("Not found");
    EXPECT_TRUE(strstr(result.c_str(), "\"error\": \"Not found\"") != nullptr);
}

TEST(ApiResponsesTest, SuccessResponse) {
    String result = ApiResponses::successResponse("Operation complete");
    EXPECT_TRUE(strstr(result.c_str(), "\"success\": true") != nullptr);
    EXPECT_TRUE(strstr(result.c_str(), "\"message\": \"Operation complete\"") != nullptr);
}