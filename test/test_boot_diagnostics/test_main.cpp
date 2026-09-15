/**
 * @file test_main.cpp
 * @brief Native tests for boot reset-reason mapping (no IDF)
 */

#include <gtest/gtest.h>

#include "../test_support.h"
#include "clevercoffee/diagnostics/BootDiagnostics.h"

TEST(BootDiagnostics, ResetReasonToStringMapsKnownValues) {
    EXPECT_STREQ(CleverCoffee::resetReasonToString(0), "unknown");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(1), "power-on");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(2), "external reset pin");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(3), "software restart");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(4), "panic or unhandled exception");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(5), "interrupt watchdog");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(6), "task watchdog");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(7), "other watchdog");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(8), "wake from deep sleep");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(9), "brownout");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(10), "SDIO");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(99), "unknown");

    EXPECT_STREQ(CleverCoffee::resetReasonToString(static_cast<int>(CleverCoffee::ResetReason::PowerOn)),
                 "power-on");
    EXPECT_STREQ(CleverCoffee::resetReasonToString(static_cast<int>(CleverCoffee::ResetReason::Panic)),
                 "panic or unhandled exception");
}

TEST(BootDiagnostics, CrashInfoIsNoneBeforeCapture) {
    EXPECT_STREQ(CleverCoffee::BootDiagnostics::crashInfoString(), "none");
}

TEST(BootDiagnostics, ResetReasonStringIsUnknownBeforeCapture) {
    EXPECT_STREQ(CleverCoffee::BootDiagnostics::resetReasonString(), "unknown");
}
