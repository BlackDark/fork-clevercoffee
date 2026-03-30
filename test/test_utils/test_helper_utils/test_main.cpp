/**
 * @file test_main.cpp
 * @brief Unit tests for custom helper utility functions
 *
 * Tests custom logic for:
 * - mod() function that handles negative numbers correctly
 * - round2() function for rounding to 2 decimal places
 * - number2string() thread-safe string conversions
 */

#include <gtest/gtest.h>
#include "../../test_support.h"
#include "clevercoffee/utils/helperUtils.h"
#include <cmath>

/**
 * @brief Test mod function with negative numbers
 *
 * The mod() function is custom logic that handles negative numbers correctly,
 * unlike C++'s % operator which can return negative results.
 */
TEST(HelperUtilsTest, ModHandlesNegativeNumbers) {
    // Test positive numbers (same as % operator)
    EXPECT_EQ(2, mod(5, 3));
    EXPECT_EQ(0, mod(6, 3));
    EXPECT_EQ(1, mod(7, 3));

    // Test negative numbers (custom logic - % gives negative results)
    EXPECT_EQ(1, mod(-2, 3));  // -2 % 3 = -2 in C++, but we want 1
    EXPECT_EQ(2, mod(-1, 3));  // -1 % 3 = -1 in C++, but we want 2
    EXPECT_EQ(0, mod(-3, 3));  // -3 % 3 = 0

    // Test edge cases
    EXPECT_EQ(0, mod(0, 5));

    // Test with various divisors
    EXPECT_EQ(0, mod(10, 5));
    EXPECT_EQ(3, mod(13, 5));
    EXPECT_EQ(0, mod(20, 10));
    EXPECT_EQ(5, mod(25, 10));
}

/**
 * @brief Test round2 function for rounding to 2 decimal places
 */
TEST(HelperUtilsTest, Round2ToTwoDecimalPlaces) {
    // Basic rounding
    EXPECT_DOUBLE_EQ(95.12, round2(95.123456));
    EXPECT_DOUBLE_EQ(95.13, round2(95.126789));
    EXPECT_DOUBLE_EQ(95.00, round2(95.0));
    EXPECT_DOUBLE_EQ(0.00, round2(0.0));
    EXPECT_DOUBLE_EQ(-95.12, round2(-95.123456));

    // Test precision edge cases (round half to even)
    EXPECT_DOUBLE_EQ(95.12, round2(95.124));
    EXPECT_DOUBLE_EQ(95.13, round2(95.125));   // Exactly halfway, rounds up
    EXPECT_DOUBLE_EQ(95.13, round2(95.126));

    // Test very small numbers
    EXPECT_DOUBLE_EQ(0.01, round2(0.005));
    EXPECT_DOUBLE_EQ(0.01, round2(0.014));
}

/**
 * @brief Test number2string for double (thread-safe conversion)
 */
TEST(HelperUtilsTest, Number2StringDouble) {
    char* result = number2string(95.12);
    EXPECT_STREQ("95.12", result);

    result = number2string(0.0);
    EXPECT_STREQ("0.00", result);

    result = number2string(100.5);
    EXPECT_STREQ("100.50", result);
}

/**
 * @brief Test number2string for int (thread-safe conversion)
 */
TEST(HelperUtilsTest, Number2StringInt) {
    char* result = number2string(95);
    EXPECT_STREQ("95", result);

    result = number2string(0);
    EXPECT_STREQ("0", result);

    result = number2string(-95);
    EXPECT_STREQ("-95", result);
}
