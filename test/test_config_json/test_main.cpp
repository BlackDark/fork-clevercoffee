/**
 * @file test_main.cpp
 * @brief Unit tests for nested config JSON helpers
 */

#include <gtest/gtest.h>

#include <ArduinoJson.h>
#include <string>

#include "../test_support.h"
#include "../../src/ConfigJson.cpp"

using CleverCoffee::ConfigJson::getNested;
using CleverCoffee::ConfigJson::setNested;
using CleverCoffee::ConfigJson::usesFlatDotKeys;

TEST(ConfigJsonTest, DetectsFlatTopLevelKeys) {
    JsonDocument doc;
    JsonObject   flat = doc["flat"].to<JsonObject>();
    flat["brew.setpoint"] = 95.0;

    JsonObject nestedOnly = doc["nested"].to<JsonObject>();
    JsonObject brew       = nestedOnly["brew"].to<JsonObject>();
    brew["setpoint"]      = 95.0;

    EXPECT_TRUE(usesFlatDotKeys(flat));
    EXPECT_FALSE(usesFlatDotKeys(nestedOnly));
}

TEST(ConfigJsonTest, SetAndGetNestedPath) {
    JsonDocument doc;
    JsonObject   root = doc.to<JsonObject>();

    {
        JsonDocument valueDoc;
        valueDoc.set("Cappuxinno");
        ASSERT_TRUE(setNested(root, "system.wifi.ssid", valueDoc.as<JsonVariant>()));
    }
    {
        JsonDocument valueDoc;
        valueDoc.set(28);
        ASSERT_TRUE(setNested(root, "brew.by_time.target_time", valueDoc.as<JsonVariant>()));
    }

    JsonVariantConst ssid = getNested(root, "system.wifi.ssid");
    ASSERT_FALSE(ssid.isNull());
    EXPECT_STREQ(ssid.as<const char*>(), "Cappuxinno");

    JsonVariantConst targetTime = getNested(root, "brew.by_time.target_time");
    ASSERT_FALSE(targetTime.isNull());
    EXPECT_EQ(targetTime.as<int>(), 28);
}

TEST(ConfigJsonTest, PreservesSiblingKeysUnderSharedParents) {
    JsonDocument doc;
    JsonObject   root = doc.to<JsonObject>();

    struct PathValue {
        const char* path;
        int         value;
    };
    const PathValue entries[] = {
        {"brew.setpoint", 95},
        {"brew.mode", 1},
        {"brew.by_time.enabled", 1},
        {"brew.by_time.target_time", 28},
        {"brew.pre_infusion.pause", 5},
        {"pid.enabled", 1},
        {"pid.regular.kp", 50},
        {"system.wifi.ssid", 99}, // sentinel replaced below
    };

    for (const auto& entry : entries) {
        JsonDocument valueDoc;
        if (entry.value == 99) {
            valueDoc.set("Cappuxinno");
        } else {
            valueDoc.set(entry.value);
        }
        ASSERT_TRUE(setNested(root, entry.path, valueDoc.as<JsonVariant>()));
    }

    EXPECT_DOUBLE_EQ(getNested(root, "brew.setpoint").as<double>(), 95.0);
    EXPECT_EQ(getNested(root, "brew.by_time.target_time").as<int>(), 28);
    EXPECT_EQ(getNested(root, "brew.pre_infusion.pause").as<int>(), 5);
    EXPECT_STREQ(getNested(root, "system.wifi.ssid").as<const char*>(), "Cappuxinno");

    std::string output;
    ASSERT_GT(serializeJsonPretty(doc, output), 0U);
    EXPECT_GT(output.length(), 200U);
}

TEST(ConfigJsonTest, GetNestedReturnsNullForMissingPath) {
    JsonDocument doc;
    JsonObject   root = doc.to<JsonObject>();
    JsonObject   brew = root["brew"].to<JsonObject>();
    brew["setpoint"]  = 95.0;

    EXPECT_FALSE(getNested(root, "brew.setpoint").isNull());
    EXPECT_DOUBLE_EQ(getNested(root, "brew.setpoint").as<double>(), 95.0);
    EXPECT_TRUE(getNested(root, "brew.missing").isNull());
    EXPECT_TRUE(getNested(root, "missing.setpoint").isNull());
}
